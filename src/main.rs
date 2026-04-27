mod db_loader;
mod query_6;
// use mongodb::bson::doc;

static CONN_STRING: &str = "mongodb://string";

#[tokio::main(flavor = "multi_thread")]
async fn main() {
  // Example usage:
  let coll = db_loader::connect(CONN_STRING)
    .await
    .unwrap()
    .database("tweets")
    .collection("tweets");

  // let count = coll.count_documents(doc! {}).await.expect("count failed");
  // println!("Total documents in collection: {}", count);

  // let total = db_loader::load_json_files(coll, "data").await.unwrap();
  // println!("Inserted {total} documents");

  let user_tweet_dists = query_6::get(&coll).await;
  for r in user_tweet_dists.iter().take(5) {
    println!("{:?}", r);
  }
}
