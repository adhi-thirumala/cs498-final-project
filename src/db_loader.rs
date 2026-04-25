use mongodb::Collection;
use mongodb::bson::Document;
use std::path::Path;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::task::JoinSet;

/// How many documents to buffer before firing a bulk insert.
const BATCH_SIZE: usize = 1000;

/// Error type for JSON loading operations.
#[derive(Debug)]
pub enum LoadError {
  Io(std::io::Error),
  Json(serde_json::Error),
  Mongo(mongodb::error::Error),
  InvalidDocument,
}

impl std::fmt::Display for LoadError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      LoadError::Io(e) => write!(f, "IO error: {e}"),
      LoadError::Json(e) => write!(f, "JSON parse error: {e}"),
      LoadError::Mongo(e) => write!(f, "MongoDB error: {e}"),
      LoadError::InvalidDocument => write!(f, "Parsed JSON is not a BSON document"),
    }
  }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
  fn from(e: std::io::Error) -> Self {
    LoadError::Io(e)
  }
}

impl From<serde_json::Error> for LoadError {
  fn from(e: serde_json::Error) -> Self {
    LoadError::Json(e)
  }
}

impl From<mongodb::error::Error> for LoadError {
  fn from(e: mongodb::error::Error) -> Self {
    LoadError::Mongo(e)
  }
}

impl From<mongodb::bson::ser::Error> for LoadError {
  fn from(e: mongodb::bson::ser::Error) -> Self {
    LoadError::Mongo(mongodb::error::Error::from(e))
  }
}

/// Loads all `.json` files from `data_dir` into the given MongoDB collection in parallel.
///
/// Each file is treated as **newline-delimited JSON** (one JSON object per line).
/// Files are read with a buffered stream so multi-GB files do not blow up RAM.
/// One `tokio` task is spawned per file; within each task documents are parsed line-by-line
/// and inserted in batches of [`BATCH_SIZE`].
///
/// Returns the total number of documents inserted, or the first error encountered.
pub async fn load_json_files(
  collection: Collection<Document>,
  data_dir: impl AsRef<Path>,
) -> Result<usize, LoadError> {
  let data_dir = data_dir.as_ref();
  let mut entries = fs::read_dir(data_dir).await?;
  let mut set = JoinSet::new();

  while let Some(entry) = entries.next_entry().await? {
    let path = entry.path();
    if path.extension().and_then(|e| e.to_str()) == Some("json") {
      let coll = collection.clone();
      set.spawn(async move { load_single_file(coll, path).await });
    }
  }

  let mut total = 0usize;
  while let Some(res) = set.join_next().await {
    total +=
      res.map_err(|e| LoadError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))??;
  }

  Ok(total)
}

/// Same as [`load_json_files`] but returns a [`Vec`] of `(file_path, result)` so you can
/// inspect per-file success / failure instead of failing fast on the first error.
pub async fn load_json_files_with_results(
  collection: Collection<Document>,
  data_dir: impl AsRef<Path>,
) -> Vec<(std::path::PathBuf, Result<usize, LoadError>)> {
  let data_dir = data_dir.as_ref();
  let Ok(mut entries) = fs::read_dir(data_dir).await else {
    return Vec::new();
  };

  let mut set = JoinSet::new();
  while let Ok(Some(entry)) = entries.next_entry().await {
    let path = entry.path();
    if path.extension().and_then(|e| e.to_str()) == Some("json") {
      let coll = collection.clone();
      set.spawn(async move {
        let result = load_single_file(coll, &path).await;
        (path, result)
      });
    }
  }

  let mut results = Vec::new();
  while let Some(res) = set.join_next().await {
    match res {
      Ok(tuple) => results.push(tuple),
      Err(e) => {
        results.push((
          std::path::PathBuf::from("<unknown>"),
          Err(LoadError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e,
          ))),
        ));
      }
    }
  }

  results
}

/// Reads one NDJSON file line-by-line and bulk-inserts into MongoDB.
async fn load_single_file(
  collection: Collection<Document>,
  path: impl AsRef<Path>,
) -> Result<usize, LoadError> {
  let file = fs::File::open(path.as_ref()).await?;
  let reader = BufReader::new(file);
  let mut lines = reader.lines();

  let mut batch = Vec::with_capacity(BATCH_SIZE);
  let mut total = 0usize;

  while let Some(line) = lines.next_line().await? {
    let line = line.trim();
    if line.is_empty() {
      continue;
    }

    let doc = line_to_doc(line)?;
    batch.push(doc);

    if batch.len() >= BATCH_SIZE {
      let docs = std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE));
      collection.insert_many(docs).await?;
      total += BATCH_SIZE;
    }
  }

  if !batch.is_empty() {
    total += batch.len();
    collection.insert_many(batch).await?;
  }

  Ok(total)
}

/// Parse a single NDJSON line into a `Document`.
fn line_to_doc(line: &str) -> Result<Document, LoadError> {
  let value: serde_json::Value = serde_json::from_str(line)?;
  match value {
    serde_json::Value::Object(map) => {
      let bson_val = mongodb::bson::to_bson(&map)?;
      match bson_val {
        mongodb::bson::Bson::Document(doc) => Ok(doc),
        _ => Err(LoadError::InvalidDocument),
      }
    }
    _ => Err(LoadError::InvalidDocument),
  }
}

/// Convenience helper: create a MongoDB client from a connection string.
pub async fn connect(uri: &str) -> Result<mongodb::Client, mongodb::error::Error> {
  mongodb::Client::with_uri_str(uri).await
}
