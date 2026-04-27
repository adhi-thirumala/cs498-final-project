use futures::TryStreamExt;
use mongodb::Collection;
use mongodb::bson::{Document, doc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UserTweetDist {
  screen_name: String,
  total_tweets: i64,
  simple_tweet_percent: f64,
  reply_percent: f64,
  retweet_percent: f64,
  quote_percent: f64,
}

pub async fn get(collection: &Collection<Document>) -> Vec<UserTweetDist> {
  println!("Query 6 (tweet dist per user) starting");

  // build a pipeline (Vec of doc!s, will be executed in sequence)
  let pipeline = vec![
    doc! {
        // filter to only verified users
        "$match": {
            "user.verified": true
        }
    },
    doc! {
        // group - group the tweets by username and count up totals
        "$group": {
            "_id": "$user.screen_name",
            "total_tweets": { "$sum": 1 },
            // create counter variables for each category; for each tweet
            // add 1 to the counter if the tweet fits that category
            // simple-tweet is signified by the absence of all other flags
            "simple_tweet_count": { "$sum": { "$cond": [{ "$and": [
                { "$eq": ["$in_reply_to_status_id", null] },
                { "$eq": [{ "$type": "$retweeted_status" }, "missing"] },
                { "$not": ["$is_quote_status"] }
            ]}, 1, 0] } },
            "reply_count": { "$sum": { "$cond": [{ "$ne": ["$in_reply_to_status_id", null] }, 1, 0] } },
            "retweet_count": { "$sum": { "$cond": [{ "$ne": [{ "$type": "$retweeted_status" }, "missing"] }, 1, 0] } },
            "quote_count": { "$sum": { "$cond": ["$is_quote_status", 1, 0] } }
        }
    },
    doc! {
        // project - do the math to put the counted data into the desired format
        "$project": {
            // include the username and the total number of tweets they made
            "screen_name": "$_id",
            "total_tweets": 1,
            // calculate the percent for each tweet category
            "simple_tweet_percent": { "$multiply": [{ "$divide": ["$simple_tweet_count", "$total_tweets"] }, 100] },
            "reply_percent": { "$multiply": [{ "$divide": ["$reply_count", "$total_tweets"] }, 100] },
            "retweet_percent": { "$multiply": [{ "$divide": ["$retweet_count", "$total_tweets"] }, 100] },
            "quote_percent": { "$multiply": [{ "$divide": ["$quote_count", "$total_tweets"] }, 100] }
        }
    },
  ];

  // run the aggregate command with the pipeline build, get the result as Cursor<Document>
  let cursor = collection
    .aggregate(pipeline)
    .with_type::<UserTweetDist>()
    .await
    .expect("Failed to get the aggregation query");

  let results: Vec<UserTweetDist> = cursor
    .try_collect()
    .await
    .expect("Failed to collect results from cursor");

  // let count = results.len() as f64;

  // let avg_simple = results.iter().map(|r| r.simple_tweet_percent).sum::<f64>() / count;
  // let avg_reply = results.iter().map(|r| r.reply_percent).sum::<f64>() / count;
  // let avg_retweet = results.iter().map(|r| r.retweet_percent).sum::<f64>() / count;
  // let avg_quote = results.iter().map(|r| r.quote_percent).sum::<f64>() / count;

  // println!("Averages across {} users:", results.len());
  // println!("  Simple: {:.2}%", avg_simple);
  // println!("  Reply:  {:.2}%", avg_reply);
  // println!("  Retweet:{:.2}%", avg_retweet);
  // println!("  Quote:  {:.2}%", avg_quote);

  println!("Query 6 (tweet dist per user) finished!");

  return results;
}
