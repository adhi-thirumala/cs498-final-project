use futures::TryStreamExt;
use mongodb::{
  Collection,
  bson::{Document, doc},
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Reply {
  pub text: String,
  pub time: String,
  pub id: i64,
  pub in_reply_to: i64,
  pub screen_name: String,
  pub user_name: String,
}

pub async fn get(collection: &Collection<Document>) -> Vec<Reply> {
  println!("Query (replies by blcklcfr) starting");

  let pipeline = vec![
    doc! {
        "$match": {
            "user.screen_name": "blcklcfr",
            "in_reply_to_status_id": { "$ne": null }
        }
    },
    doc! {
        "$project": {
            "_id": 0,
            "text": 1,
            "time": "$created_at",
            "id": 1,
            "in_reply_to": "$in_reply_to_status_id",
            "screen_name": "$user.screen_name",
            "user_name": "$user.name"
        }
    },
  ];

  let cursor = collection
    .aggregate(pipeline)
    .with_type::<Reply>()
    .await
    .expect("Failed to get the aggregation query");

  let results: Vec<Reply> = cursor
    .try_collect()
    .await
    .expect("Failed to collect results from cursor");

  println!("Query (replies by blcklcfr) finished!");

  results
}
