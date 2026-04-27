use futures::TryStreamExt;
use mongodb::Collection;
use mongodb::bson::{Document, doc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct HashtagCount {
  hashtag: String,
  count: i64,
}

pub async fn get(collection: &Collection<Document>) -> Vec<HashtagCount> {
  println!("Query 4 (tweets per hashtag) starting");

  // build a pipeline (Vec of doc!s, will be executed in sequence)
  let pipeline = vec![
    doc! {
        // split each tweet with multiple hashtags
        // into multiple tweets with one hashtag each
        // so that we can just run an aggregate->count query on it
        "$unwind": "$entities.hashtags"
    },
    doc! {
        // count each hashtag, then sort
        "$group": {
            "_id": { "$toLower": "$entities.hashtags.text" },
            "count": { "$sum": 1 }
        }

    },
    doc! {
        "$sort": { "count": -1 }
    },
    doc! {
        // keep only the first 100 results
        "$limit": 100
    },
    doc! {
        // project so that we have the values as we want them
        "$project": {
            "_id": 0,
            "hashtag": "$_id",
            "count": 1
        }
    },
  ];

  // run the aggregate command with the pipeline build, get the result as Cursor<Document>
  let cursor = collection
    .aggregate(pipeline)
    .with_type::<HashtagCount>()
    .await
    .expect("Failed to get the aggregation query");

  let results: Vec<HashtagCount> = cursor
    .try_collect()
    .await
    .expect("Failed to collect results from cursor");

  println!("Query 4 (tweets per hashtag) finished!");

  return results;
}
