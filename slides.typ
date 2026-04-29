#import "@preview/touying:0.6.3": *
#import themes.metropolis: *

#show: metropolis-theme.with(
  aspect-ratio: "16-9",
  config-info(
    title: [Tweet Query Project],
    subtitle: [Querying 6.7 GB of Eurovision tweets in MongoDB],
    author: [Adhi Thirumala #sym.dot.c Quinten Schafer #sym.dot.c Ashley Li],
    date: datetime.today(),
    institution: [CS498 DCU - Final Project],
  ),
)

// Override metropolis' default 20pt body text to leave room for code.
#set text(size: 16pt)
#set par(leading: 0.55em)
#show raw: set text(font: "Fira Mono")
#show raw.where(block: true): set par(leading: 0.4em)

// ---------------------------------------------------------------------------
// Helper: pull a contiguous slice of lines out of a source file by line range.
// `from` and `to` are 1-indexed and inclusive, matching the editor gutter.
// ---------------------------------------------------------------------------
#let extract-lines(path, from, to) = {
  let lines = read(path).split("\n")
  lines.slice(from - 1, to).join("\n")
}

// Tight code-block wrapper so explanations and code share one slide.
#let codeblock(size, body) = block(
  width: 100%,
  inset: (top: 0.3em, bottom: 0.3em),
  text(size: size, body),
)

#title-slide()

// ===========================================================================
= The Data Model
// ===========================================================================

== One Tweet, Trimmed to What We Query

Raw NDJSON loaded straight into Mongo. Fields not used by any query elided.

#codeblock(8pt)[```js
{
  _id: ObjectId('69ec115d586647f6e105e82d'),
  created_at: 'Sat May 12 22:22:30 +0000 2018',
  id: Long('995429034791235585'),
  text: '#Eurovision pero que ha hecho Israel ...',
  in_reply_to_status_id: null,
  in_reply_to_screen_name: null,
  user: {
    id_str: '2845826980',           name: 'sol maria',
    screen_name: 'solmariacv',      location: 'PIURA',
    description: 'UNA SIMPLE PULPINA, ...',
    verified: false,                lang: 'es',
    followers_count: Long('90'),    friends_count: Long('443'),
    statuses_count: Long('1359'),   time_zone: null,
    created_at: 'Sun Oct 26 21:18:14 +0000 2014',
    ...
  },
  place:    { country: 'Peru', ... },
  entities: { hashtags: [ { text: 'Eurovision', ... } ], ... },
  is_quote_status: false,
  // retweeted_status: { ... }   // present only if the tweet is a retweet
  ...
}
```]

// ===========================================================================
= Demonstration
// ===========================================================================

#focus-slide[
  Live demo
  #v(0.5em)
  #text(size: 22pt)[Mongo queries running against the loaded collection.]
]

// ===========================================================================
= The Six Queries
// ===========================================================================

== Top User - `user_with_most_tweets.rs`

Filter to docs with a string `screen_name`, count by author, take the leader.

#codeblock(11pt)[#raw(
  extract-lines("src/user_with_most_tweets.rs", 14, 23),
  lang: "rust",
  block: true,
)]

`$sortByCount` is sugar for `$group + $sort`: each distinct value paired with
its document count, sorted descending.

== Top Country - `country_with_most_tweets.rs`

Same shape as the top-user query, but pivots on `place.country`.

#codeblock(11pt)[#raw(
  extract-lines("src/country_with_most_tweets.rs", 14, 23),
  lang: "rust",
  block: true,
)]

The `$type: "string"` guard skips tweets with no `place` (most of them).

== Tweets per Hashtag - `tweets_per_hashtag.rs`

`$unwind` explodes the hashtag array; `$group` lower-cases and counts.

#codeblock(8pt)[#raw(
  extract-lines("src/tweets_per_hashtag.rs", 17, 47),
  lang: "rust",
  block: true,
)]

== Replies by `@blcklcfr` - `replies_by_blcklcfr.rs`

Pin `screen_name`, require a non-null `in_reply_to_status_id`, flatten output.

#codeblock(10pt)[#raw(
  extract-lines("src/replies_by_blcklcfr.rs", 22, 40),
  lang: "rust",
  block: true,
)]

== Verified Tweet Mix - `$match + $group`

Filter to verified users, group by author, count each tweet category.

#codeblock(8pt)[#raw(
  extract-lines("src/user_tweet_dist.rs", 21, 45),
  lang: "rust",
  block: true,
)]

A "simple" tweet is the absence of every other flag. Retweets are detected
via `$type ... "missing"` on `retweeted_status`.

== Verified Tweet Mix - `$project`

Each counter divided by `total_tweets` gives the percentage breakdown.

#codeblock(9pt)[#raw(
  extract-lines("src/user_tweet_dist.rs", 46, 60),
  lang: "rust",
  block: true,
)]

== Mutual-Reply Triangles (1/2) - build edges

Collapse every reply into a deduplicated directed edge `(from, to)`.

#codeblock(11pt)[#raw(
  extract-lines("src/triangle.rs", 57, 70),
  lang: "rust",
  block: true,
)]

Triangle detection itself happens in Rust over the returned graph.

== Mutual-Reply Triangles (2/2) - hydrate users

Pull every screen name in a triangle; `$group` keeps one profile per author.

#codeblock(7pt)[#raw(
  extract-lines("src/triangle.rs", 122, 153),
  lang: "rust",
  block: true,
)]

// ===========================================================================
= Loading the Dataset
// ===========================================================================

== One `insert_many` at a Time

8 files, ~6.7 GB, loaded verbatim. One Tokio task per file, batches of 1 000.

#codeblock(7pt)[#raw(
  extract-lines("src/db_loader.rs", 137, 166),
  lang: "rust",
  block: true,
)]

// ===========================================================================
= Critique
// ===========================================================================

== What We'd Do Differently

- *Clean the data on the way in.* Half of every document is ceremony we never
  read (profile colors, theme URLs, follow flags). Stripping it at load time
  would have shrunk the working set and sped up every scan.
- *More indexes.* We only added an index on `user.screen_name`. The hashtag
  unwind, the `place.country` scan, and the triangle edge build each had to
  read every document.
- *Change from the original design.* We had planned a cleaning pass; in
  practice we skipped it and pushed raw tweets straight into Mongo.
- *Hosting choice.* We spun up a GCP VM and ran `mongod` ourselves instead
  of using a managed cluster.

// ===========================================================================
= Lessons Learned
// ===========================================================================

== Self-Hosting MongoDB Is Not Free

- Managing a database at this scale by hand is its own project.
- The newest `mongod` shipped a bundled `tcmalloc` that refused to start on
  our VM - a libc-related flag in `mongod.conf` was needed before
  it would even boot.
- A single node was already painful; a real deployment also needs replica
  sets, backups, and failover - each one its own rabbit hole.
- *Takeaway:* unless ops is the point, use Atlas (or any managed service).
  Self-hosting Mongo is needlessly complicated for a class project.

#focus-slide[
  Questions?
]
