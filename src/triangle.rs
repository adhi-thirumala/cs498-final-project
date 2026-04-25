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

#[derive(Debug, Deserialize)]
pub(super) struct User {
  pub name: String,
  pub screen_name: String,
}

#[derive(Debug, Deserialize, Eq, Hash, PartialEq, Clone)]
struct Pair {
  from: String,
  to: String,
}

#[derive(Debug, Deserialize)]
struct NameDoc {
  screen_name: String,
  name: String,
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

  let names_pipeline = vec![
    doc! { "$match": { "user.screen_name": { "$in": all_screen_names.into_iter().collect::<Vec<String>>() } } },
    doc! { "$group": {
        "_id": "$user.screen_name",
        "name": { "$first": "$user.name" }
    }},
    doc! { "$project": {
        "_id": 0,
        "screen_name": "$_id",
        "name": 1
    }},
  ];

  let name_docs = collection
    .aggregate(names_pipeline)
    .with_type::<NameDoc>()
    .await
    .expect("names query shouldnt fail")
    .try_collect::<Vec<NameDoc>>()
    .await
    .expect("names collect shouldnt fail");

  let name_map: HashMap<String, String> = name_docs
    .into_iter()
    .map(|d| (d.screen_name, d.name))
    .collect();

  triangles
    .into_iter()
    .map(|(a, b, c)| Triple {
      a: User {
        name: name_map.get(&a).cloned().unwrap_or_default(),
        screen_name: a,
      },
      b: User {
        name: name_map.get(&b).cloned().unwrap_or_default(),
        screen_name: b,
      },
      c: User {
        name: name_map.get(&c).cloned().unwrap_or_default(),
        screen_name: c,
      },
    })
    .collect()
}
