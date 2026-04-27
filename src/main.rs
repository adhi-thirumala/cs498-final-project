mod db_loader;
mod tweets_per_hashtag;
mod user_tweet_dist;
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

  let results = tweets_per_hashtag::get(&coll).await;

  println!("Collected {:?} results", results.len());

  for r in results.iter().take(5) {
    println!("{:?}", r);
  }
}
