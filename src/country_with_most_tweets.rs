use futures::stream::TryStreamExt;
use mongodb::{
  Collection,
  bson::{Document, doc},
};

#[derive(Debug)]
pub struct CountryTweets {
  pub country: String,
  pub count: i64,
}

pub async fn get(collection: Collection<Document>) -> Result<CountryTweets, mongodb::error::Error> {
  let pipeline = vec![
    doc! { "$match": { "place.country": { "$type": "string" } } },
    doc! { "$sortByCount": "$place.country" },
    doc! { "$limit": 1 },
    doc! { "$project": {
        "_id": 0,
        "country": "$_id",
        "count": { "$toLong": "$count" },
    }},
  ];

  let mut cursor = collection.aggregate(pipeline).await?;
  match cursor.try_next().await {
    Ok(Some(doc)) => {
      let country = doc.get_str("country").expect("missing country").to_string();
      let count = doc.get_i64("count").expect("missing count");
      Ok(CountryTweets { country, count })
    }
    Ok(None) => panic!("aggregation returned no documents"),
    Err(e) => Err(e),
  }
}
