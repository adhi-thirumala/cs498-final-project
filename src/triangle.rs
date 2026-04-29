use std::collections::{HashMap, HashSet};

use mongodb::{
  Collection,
  bson::{Document, doc},
};

use futures::TryStreamExt;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct Triple {
  pub a: User,
  pub b: User,
  pub c: User,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct User {
  pub name: String,
  pub screen_name: String,
  pub id_str: Option<String>,
  pub location: Option<String>,
  pub description: Option<String>,
  pub verified: Option<bool>,
  pub followers_count: Option<i64>,
  pub friends_count: Option<i64>,
  pub statuses_count: Option<i64>,
  pub created_at: Option<String>,
  pub lang: Option<String>,
  pub time_zone: Option<String>,
}

#[derive(Debug, Deserialize, Eq, Hash, PartialEq, Clone)]
struct Pair {
  from: String,
  to: String,
}

#[derive(Debug, Deserialize)]
struct UserDoc {
  screen_name: String,
  name: String,
  id_str: Option<String>,
  location: Option<String>,
  description: Option<String>,
  verified: Option<bool>,
  followers_count: Option<i64>,
  friends_count: Option<i64>,
  statuses_count: Option<i64>,
  created_at: Option<String>,
  lang: Option<String>,
  time_zone: Option<String>,
}

pub async fn triangle(collection: Collection<Document>) -> Vec<Triple> {
  let pipeline = vec![
    doc! { "$match": { "in_reply_to_screen_name": { "$ne": null } } },
    doc! { "$group": {
        "_id": {
            "from": "$user.screen_name",
            "to":   "$in_reply_to_screen_name",
        }
    }},
    doc! { "$project": {
        "_id": 0,
        "from": "$_id.from",
        "to":   "$_id.to",
    }},
  ];
  let edges = collection
    .aggregate(pipeline)
    .with_type::<Pair>()
    .await
    .expect("shouldnt fail here at line 37 of triangle")
    .try_collect::<HashSet<Pair>>()
    .await
    .expect("colelct shouldnt fail either");

  let mutual: HashSet<(String, String)> = edges
    .iter()
    .filter(|p| {
      p.from < p.to
        && edges.contains(&Pair {
          from: p.to.clone(),
          to: p.from.clone(),
        })
    })
    .map(|p| (p.from.clone(), p.to.clone()))
    .collect();

  let adj: HashMap<String, HashSet<String>> =
    mutual.iter().fold(HashMap::new(), |mut acc, (a, b)| {
      acc.entry(a.clone()).or_default().insert(b.clone());
      acc.entry(b.clone()).or_default().insert(a.clone());
      acc
    });
  let triangles: Vec<(String, String, String)> = mutual
    .iter()
    .flat_map(|(a, b)| {
      let a = a.clone();
      let b = b.clone();
      adj[&a]
        .intersection(&adj[&b])
        .cloned()
        .filter(|c| &b < c) // a<b already guaranteed
        .map(|c| (a.clone(), b.clone(), c))
        .collect::<Vec<_>>()
    })
    .collect();

  let all_screen_names: HashSet<String> = triangles
    .iter()
    .flat_map(|(a, b, c)| [a, b, c])
    .cloned()
    .collect();

  if all_screen_names.is_empty() {
    return Vec::new();
  }

  let users_pipeline = vec![
    doc! { "$match": { "user.screen_name": { "$in": all_screen_names.into_iter().collect::<Vec<String>>() } } },
    doc! { "$group": {
        "_id": "$user.screen_name",
        "name": { "$first": "$user.name" },
        "id_str": { "$first": "$user.id_str" },
        "location": { "$first": "$user.location" },
        "description": { "$first": "$user.description" },
        "verified": { "$first": "$user.verified" },
        "followers_count": { "$first": "$user.followers_count" },
        "friends_count": { "$first": "$user.friends_count" },
        "statuses_count": { "$first": "$user.statuses_count" },
        "created_at": { "$first": "$user.created_at" },
        "lang": { "$first": "$user.lang" },
        "time_zone": { "$first": "$user.time_zone" }
    }},
    doc! { "$project": {
        "_id": 0,
        "screen_name": "$_id",
        "name": 1,
        "id_str": 1,
        "location": 1,
        "description": 1,
        "verified": 1,
        "followers_count": 1,
        "friends_count": 1,
        "statuses_count": 1,
        "created_at": 1,
        "lang": 1,
        "time_zone": 1
    }},
  ];

  let user_docs = collection
    .aggregate(users_pipeline)
    .with_type::<UserDoc>()
    .await
    .expect("users query shouldnt fail")
    .try_collect::<Vec<UserDoc>>()
    .await
    .expect("users collect shouldnt fail");

  let user_map: HashMap<String, User> = user_docs
    .into_iter()
    .map(|d| {
      let screen_name = d.screen_name;
      (
        screen_name.clone(),
        User {
          name: d.name,
          screen_name,
          id_str: d.id_str,
          location: d.location,
          description: d.description,
          verified: d.verified,
          followers_count: d.followers_count,
          friends_count: d.friends_count,
          statuses_count: d.statuses_count,
          created_at: d.created_at,
          lang: d.lang,
          time_zone: d.time_zone,
        },
      )
    })
    .collect();

  triangles
    .into_iter()
    .map(|(a, b, c)| Triple {
      a: user_for(a, &user_map),
      b: user_for(b, &user_map),
      c: user_for(c, &user_map),
    })
    .collect()
}

fn user_for(screen_name: String, user_map: &HashMap<String, User>) -> User {
  user_map.get(&screen_name).cloned().unwrap_or(User {
    name: String::new(),
    screen_name,
    id_str: None,
    location: None,
    description: None,
    verified: None,
    followers_count: None,
    friends_count: None,
    statuses_count: None,
    created_at: None,
    lang: None,
    time_zone: None,
  })
}
