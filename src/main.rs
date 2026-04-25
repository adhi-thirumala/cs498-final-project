mod db_loader;

static CONN_STRING: &str = include_str!("../.mongostr");

#[tokio::main(flavor = "multi_thread")]
async fn main() {
  // Example usage:
  let coll = db_loader::connect(CONN_STRING)
    .await
    .unwrap()
    .database("tweets")
    .collection("main-collection");
  let total = db_loader::load_json_files(coll, "data").await.unwrap();
  println!("Inserted {total} documents");
}
