use mongodb::{ 
    bson::doc,
    bson::Document,
    Collection
};
use futures::stream::TryStreamExt;

pub async fn get(collection: Collection<Document>) -> mongodb::error::Result<()> {
    let pipeline = vec![
        doc! { "$match": { "place.country": { "$type": "string" } } },
        doc! { "$sortByCount": "$place.country" },
        doc! { "$limit": 1 }
    ];

    let mut country = collection.aggregate(pipeline).await?;
    while let Some(doc) = country.try_next().await? {
        println!("{:#?}", doc);
    }

    Ok(())
}