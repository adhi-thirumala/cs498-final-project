use mongodb::{
    bson::{doc, Document},
    Collection,
};
use futures::stream::TryStreamExt;

#[derive(Debug)]
pub struct UserTweets {
    pub screen_name: String,
    pub count: i64,
}

pub async fn get(collection: Collection<Document>) -> Result<UserTweets, mongodb::error::Error> {
    let pipeline = vec![
        doc! { "$match": { "user.screen_name": { "$type": "string" } } },
        doc! { "$sortByCount": "$user.screen_name" },
        doc! { "$limit": 1 },
        doc! { "$project": {
            "_id": 0,
            "screen_name": "$_id",
            "count": { "$toLong": "$count" },
        }},
    ];

    let mut cursor = collection.aggregate(pipeline).await?;
    match cursor.try_next().await {
        Ok(Some(doc)) => {
            let screen_name = doc.get_str("screen_name").expect("missing screen_name").to_string();
            let count = doc.get_i64("count").expect("missing count");
            Ok(UserTweets { screen_name, count })
        }
        Ok(None) => panic!("aggregation returned no documents"),
        Err(e) => Err(e),
    }
}
