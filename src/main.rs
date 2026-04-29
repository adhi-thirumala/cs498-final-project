mod country_with_most_tweets;
mod db_loader;
mod replies_by_blcklcfr;
mod triangle;
mod tweets_per_hashtag;
mod user_tweet_dist;
mod user_with_most_tweets;

use std::{error::Error, io::Stdout, time::Duration};

use crossterm::{
  event::{self, Event, KeyCode, KeyEventKind},
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use mongodb::{Collection, bson::Document};
use ratatui::{
  Frame, Terminal,
  backend::CrosstermBackend,
  buffer::Buffer,
  layout::{Alignment, Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span},
  widgets::{
    Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Wrap,
  },
};

static CONN_STRING: &str =
  "mongodb://adminUser:ilovedalegend@34.70.144.165:46767/?authSource=admin";
const QUERY_LABELS: [&str; 6] = [
  "Triangles",
  "Tweets / Hashtag",
  "Verified Tweet Mix",
  "Replies by blcklcfr",
  "Top Country",
  "Top User",
];
const PAGE_STEP: usize = 10;

type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

struct QueryData {
  country: Result<country_with_most_tweets::CountryTweets, String>,
  top_user: Result<user_with_most_tweets::UserTweets, String>,
  hashtags: Vec<tweets_per_hashtag::HashtagCount>,
  replies: Vec<replies_by_blcklcfr::Reply>,
  user_dist: Vec<user_tweet_dist::UserTweetDist>,
  triangles: Vec<triangle::Triple>,
}

struct App {
  selected_query: usize,
  offset: usize,
  triangle_index: usize,
  data: QueryData,
}

impl App {
  fn new(data: QueryData) -> Self {
    Self {
      selected_query: 0,
      offset: 0,
      triangle_index: 0,
      data,
    }
  }

  fn next_query(&mut self) {
    self.selected_query = (self.selected_query + 1) % QUERY_LABELS.len();
    self.offset = 0;
    self.triangle_index = self
      .triangle_index
      .min(self.data.triangles.len().saturating_sub(1));
  }

  fn previous_query(&mut self) {
    self.selected_query = if self.selected_query == 0 {
      QUERY_LABELS.len() - 1
    } else {
      self.selected_query - 1
    };
    self.offset = 0;
    self.triangle_index = self
      .triangle_index
      .min(self.data.triangles.len().saturating_sub(1));
  }

  fn next_page(&mut self) {
    if self.selected_query == 0 {
      self.triangle_index =
        (self.triangle_index + 1).min(self.data.triangles.len().saturating_sub(1));
      return;
    }

    let max_offset = self.item_count().saturating_sub(1);
    self.offset = (self.offset + PAGE_STEP).min(max_offset);
  }

  fn previous_page(&mut self) {
    if self.selected_query == 0 {
      self.triangle_index = self.triangle_index.saturating_sub(1);
      return;
    }

    self.offset = self.offset.saturating_sub(PAGE_STEP);
  }

  fn item_count(&self) -> usize {
    match self.selected_query {
      0 => self.data.triangles.len(),
      1 => self.data.hashtags.len(),
      2 => self.data.user_dist.len(),
      3 => self.data.replies.len(),
      4 => usize::from(self.data.country.is_ok()),
      5 => usize::from(self.data.top_user.is_ok()),
      _ => 0,
    }
  }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
  let conn_string = std::env::var("MONGODB_URI").unwrap_or_else(|_| CONN_STRING.to_string());
  let coll = db_loader::connect(&conn_string)
    .await?
    .database("tweets")
    .collection("tweets");

  // let count = coll.count_documents(doc! {}).await.expect("count failed");
  // println!("Total documents in collection: {}", count);

  // let total = db_loader::load_json_files(coll, "data").await.unwrap();
  // println!("Inserted {total} documents");

  let data = load_query_data(&coll).await;
  let mut terminal = init_terminal()?;
  let app_result = run_app(&mut terminal, App::new(data));
  restore_terminal(&mut terminal)?;

  app_result
}

async fn load_query_data(collection: &Collection<Document>) -> QueryData {
  let (country, top_user, hashtags, replies, user_dist, triangles) = tokio::join!(
    country_with_most_tweets::get(collection.clone()),
    user_with_most_tweets::get(collection.clone()),
    tweets_per_hashtag::get(collection),
    replies_by_blcklcfr::get(collection),
    user_tweet_dist::get(collection),
    triangle::triangle(collection.clone()),
  );

  QueryData {
    country: country.map_err(|err| err.to_string()),
    top_user: top_user.map_err(|err| err.to_string()),
    hashtags,
    replies,
    user_dist,
    triangles,
  }
}

fn init_terminal() -> Result<AppTerminal, Box<dyn Error>> {
  enable_raw_mode()?;
  let mut stdout = std::io::stdout();
  execute!(stdout, EnterAlternateScreen)?;
  let backend = CrosstermBackend::new(stdout);
  Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut AppTerminal) -> Result<(), Box<dyn Error>> {
  disable_raw_mode()?;
  execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
  terminal.show_cursor()?;
  Ok(())
}

fn run_app(terminal: &mut AppTerminal, mut app: App) -> Result<(), Box<dyn Error>> {
  loop {
    terminal.draw(|frame| render(frame, &app))?;

    if !event::poll(Duration::from_millis(200))? {
      continue;
    }

    let Event::Key(key) = event::read()? else {
      continue;
    };

    if key.kind != KeyEventKind::Press {
      continue;
    }

    match key.code {
      KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
      KeyCode::Down | KeyCode::Char('j') => app.next_query(),
      KeyCode::Up | KeyCode::Char('k') => app.previous_query(),
      KeyCode::Right | KeyCode::Char('l') => app.next_page(),
      KeyCode::Left | KeyCode::Char('h') => app.previous_page(),
      _ => {}
    }
  }
}

fn render(frame: &mut Frame<'_>, app: &App) {
  let root = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(3),
      Constraint::Min(12),
      Constraint::Length(3),
    ])
    .split(frame.area());

  render_header(frame, root[0], app);

  let body = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Length(28), Constraint::Min(30)])
    .split(root[1]);

  render_sidebar(frame, body[0], app);
  render_query(frame, body[1], app);
  render_footer(frame, root[2]);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
  let title = format!(
    "Tweet Query Visualizer - {} - {} rows",
    QUERY_LABELS[app.selected_query],
    app.item_count()
  );
  frame.render_widget(
    Paragraph::new(title)
      .alignment(Alignment::Center)
      .style(
        Style::default()
          .fg(Color::Yellow)
          .add_modifier(Modifier::BOLD),
      )
      .block(panel_block("CS498 Final Project").border_style(Style::default().fg(Color::Cyan))),
    area,
  );
}

fn render_sidebar(frame: &mut Frame<'_>, area: Rect, app: &App) {
  let items = QUERY_LABELS
    .iter()
    .map(|label| ListItem::new(Line::from(*label)))
    .collect::<Vec<_>>();
  let list = List::new(items)
    .block(panel_block("Queries").border_style(Style::default().fg(Color::Cyan)))
    .highlight_style(
      Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::REVERSED),
    )
    .highlight_symbol("  ");
  let mut state = ListState::default();
  state.select(Some(app.selected_query));
  frame.render_stateful_widget(list, area, &mut state);
}

fn render_query(frame: &mut Frame<'_>, area: Rect, app: &App) {
  match app.selected_query {
    0 => render_triangles(frame, area, app),
    1 => render_hashtags(frame, area, app),
    2 => render_user_dist(frame, area, app),
    3 => render_replies(frame, area, app),
    4 => render_country(frame, area, app),
    5 => render_top_user(frame, area, app),
    _ => {}
  }
}

fn render_hashtags(frame: &mut Frame<'_>, area: Rect, app: &App) {
  let visible_rows = visible_rows(area);
  let rows = app
    .data
    .hashtags
    .iter()
    .skip(app.offset)
    .take(visible_rows)
    .map(|row| {
      Row::new(vec![
        Cell::from(format!("#{}", row.hashtag)),
        Cell::from(row.count.to_string()),
      ])
    });

  let table = Table::new(rows, [Constraint::Percentage(75), Constraint::Length(12)])
    .header(table_header(["Hashtag", "Tweets"]))
    .block(panel_block(page_title(
      "Tweets Per Hashtag",
      app.offset,
      app.data.hashtags.len(),
    )))
    .column_spacing(2);
  frame.render_widget(table, area);
}

fn render_user_dist(frame: &mut Frame<'_>, area: Rect, app: &App) {
  let visible_rows = visible_rows(area);
  let rows = app
    .data
    .user_dist
    .iter()
    .skip(app.offset)
    .take(visible_rows)
    .map(|row| {
      Row::new(vec![
        Cell::from(format!("@{}", row.screen_name)),
        Cell::from(row.total_tweets.to_string()),
        Cell::from(percent(row.simple_tweet_percent)),
        Cell::from(percent(row.reply_percent)),
        Cell::from(percent(row.retweet_percent)),
        Cell::from(percent(row.quote_percent)),
      ])
    });

  let table = Table::new(
    rows,
    [
      Constraint::Percentage(32),
      Constraint::Length(8),
      Constraint::Length(10),
      Constraint::Length(9),
      Constraint::Length(9),
      Constraint::Length(9),
    ],
  )
  .header(table_header([
    "User", "Tweets", "Simple", "Reply", "Retweet", "Quote",
  ]))
  .block(panel_block(page_title(
    "Verified User Tweet Mix",
    app.offset,
    app.data.user_dist.len(),
  )))
  .column_spacing(1);
  frame.render_widget(table, area);
}

fn render_replies(frame: &mut Frame<'_>, area: Rect, app: &App) {
  let visible_rows = visible_rows(area);
  let rows = app
    .data
    .replies
    .iter()
    .skip(app.offset)
    .take(visible_rows)
    .map(|row| {
      Row::new(vec![
        Cell::from(format!("@{}", row.screen_name)),
        Cell::from(row.time.clone()),
        Cell::from(row.in_reply_to.to_string()),
        Cell::from(row.text.clone()),
      ])
    });

  let table = Table::new(
    rows,
    [
      Constraint::Length(18),
      Constraint::Length(30),
      Constraint::Length(14),
      Constraint::Percentage(50),
    ],
  )
  .header(table_header(["User", "Time", "Reply To", "Text"]))
  .block(panel_block(page_title(
    "Replies by blcklcfr",
    app.offset,
    app.data.replies.len(),
  )))
  .column_spacing(1);
  frame.render_widget(table, area);
}

fn render_country(frame: &mut Frame<'_>, area: Rect, app: &App) {
  match &app.data.country {
    Ok(country) => render_metric(
      frame,
      area,
      "Country With Most Tweets",
      vec![
        Line::from(vec![
          Span::styled("Country: ", label_style()),
          Span::raw(&country.country),
        ]),
        Line::from(vec![
          Span::styled("Tweets:  ", label_style()),
          Span::raw(country.count.to_string()),
        ]),
      ],
    ),
    Err(err) => render_metric(
      frame,
      area,
      "Country With Most Tweets",
      vec![Line::raw(err)],
    ),
  }
}

fn render_top_user(frame: &mut Frame<'_>, area: Rect, app: &App) {
  match &app.data.top_user {
    Ok(user) => render_metric(
      frame,
      area,
      "User With Most Tweets",
      vec![
        Line::from(vec![
          Span::styled("User:   ", label_style()),
          Span::raw(format!("@{}", user.screen_name)),
        ]),
        Line::from(vec![
          Span::styled("Tweets: ", label_style()),
          Span::raw(user.count.to_string()),
        ]),
      ],
    ),
    Err(err) => render_metric(frame, area, "User With Most Tweets", vec![Line::raw(err)]),
  }
}

fn render_metric(frame: &mut Frame<'_>, area: Rect, title: &str, lines: Vec<Line<'_>>) {
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Percentage(35),
      Constraint::Length(8),
      Constraint::Percentage(35),
    ])
    .split(area);
  frame.render_widget(
    Paragraph::new(lines)
      .alignment(Alignment::Center)
      .block(panel_block(title))
      .style(Style::default())
      .wrap(Wrap { trim: true }),
    chunks[1],
  );
}

fn render_triangles(frame: &mut Frame<'_>, area: Rect, app: &App) {
  let title = if app.data.triangles.is_empty() {
    "Triangles".to_string()
  } else {
    format!(
      "Triangles {}/{}",
      app.triangle_index + 1,
      app.data.triangles.len()
    )
  };
  let block = panel_block(title);
  let inner = block.inner(area);
  frame.render_widget(block, area);

  let Some(triple) = app.data.triangles.get(app.triangle_index) else {
    frame.render_widget(
      Paragraph::new("No mutual-reply triangles returned by the query.")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray)),
      inner,
    );
    return;
  };

  if inner.width < 76 || inner.height < 24 {
    render_triangle_text_fallback(frame, inner, triple);
    return;
  }

  let node_width = 42.min((inner.width.saturating_sub(8)) / 2).max(28);
  let node_height = 10;
  let top = Rect::new(
    inner.x + (inner.width - node_width) / 2,
    inner.y + 1,
    node_width,
    node_height,
  );
  let left = Rect::new(
    inner.x + 2,
    inner.y + inner.height - node_height - 1,
    node_width,
    node_height,
  );
  let right = Rect::new(
    inner.x + inner.width - node_width - 2,
    inner.y + inner.height - node_height - 1,
    node_width,
    node_height,
  );

  let line_style = Style::default().fg(Color::DarkGray);
  draw_line(
    frame.buffer_mut(),
    inner,
    (top.x + top.width / 2, top.y + top.height),
    (left.x + left.width / 2, left.y.saturating_sub(1)),
    line_style,
  );
  draw_line(
    frame.buffer_mut(),
    inner,
    (top.x + top.width / 2, top.y + top.height),
    (right.x + right.width / 2, right.y.saturating_sub(1)),
    line_style,
  );
  draw_horizontal(
    frame.buffer_mut(),
    inner,
    left.x + left.width,
    right.x.saturating_sub(1),
    left.y + left.height / 2,
    line_style,
  );

  render_user_node(frame, top, "A", &triple.a, Color::Red);
  render_user_node(frame, left, "B", &triple.b, Color::Yellow);
  render_user_node(frame, right, "C", &triple.c, Color::Blue);
}

fn render_triangle_text_fallback(frame: &mut Frame<'_>, area: Rect, triple: &triangle::Triple) {
  let lines = [("A", &triple.a), ("B", &triple.b), ("C", &triple.c)]
    .into_iter()
    .flat_map(|(label, user)| {
      let mut lines = vec![Line::from(format_user(label, user))];
      lines.extend(user_detail_lines(user).into_iter().take(5));
      lines.push(Line::raw(""));
      lines
    })
    .collect::<Vec<_>>();
  frame.render_widget(
    Paragraph::new(lines)
      .alignment(Alignment::Center)
      .style(Style::default())
      .wrap(Wrap { trim: true }),
    area,
  );
}

fn render_user_node(
  frame: &mut Frame<'_>,
  area: Rect,
  label: &str,
  user: &triangle::User,
  color: Color,
) {
  let mut lines = vec![Line::from(Span::styled(
    format!("@{}", user.screen_name),
    Style::default().add_modifier(Modifier::BOLD),
  ))];
  lines.extend(user_detail_lines(user).into_iter().take(7));
  frame.render_widget(
    Paragraph::new(lines)
      .alignment(Alignment::Center)
      .block(panel_block(label).border_style(Style::default().fg(color)))
      .wrap(Wrap { trim: true }),
    area,
  );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect) {
  frame.render_widget(
    Paragraph::new(
      "j/k or Up/Down: select query | h/l or Left/Right: page rows / triangles | q: quit",
    )
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::Magenta))
    .block(panel_block("Keys").border_style(Style::default().fg(Color::Cyan))),
    area,
  );
}

fn draw_horizontal(buf: &mut Buffer, clip: Rect, x1: u16, x2: u16, y: u16, style: Style) {
  let (start, end) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
  for x in start..=end {
    draw_symbol(buf, clip, x, y, "─", style);
  }
}

fn draw_line(buf: &mut Buffer, clip: Rect, from: (u16, u16), to: (u16, u16), style: Style) {
  let mut x = i32::from(from.0);
  let mut y = i32::from(from.1);
  let end_x = i32::from(to.0);
  let end_y = i32::from(to.1);
  let dx = (end_x - x).abs();
  let dy = -(end_y - y).abs();
  let step_x = if x < end_x { 1 } else { -1 };
  let step_y = if y < end_y { 1 } else { -1 };
  let mut err = dx + dy;
  let symbol = if from.0 == to.0 {
    "│"
  } else if from.1 == to.1 {
    "─"
  } else if (from.0 < to.0) == (from.1 < to.1) {
    "╲"
  } else {
    "╱"
  };

  loop {
    if let (Ok(cell_x), Ok(cell_y)) = (u16::try_from(x), u16::try_from(y)) {
      draw_symbol(buf, clip, cell_x, cell_y, symbol, style);
    }

    if x == end_x && y == end_y {
      break;
    }

    let doubled_err = 2 * err;
    if doubled_err >= dy {
      err += dy;
      x += step_x;
    }
    if doubled_err <= dx {
      err += dx;
      y += step_y;
    }
  }
}

fn draw_symbol(buf: &mut Buffer, clip: Rect, x: u16, y: u16, symbol: &str, style: Style) {
  if x < clip.x
    || x >= clip.x.saturating_add(clip.width)
    || y < clip.y
    || y >= clip.y.saturating_add(clip.height)
  {
    return;
  }
  buf[(x, y)].set_symbol(symbol).set_style(style);
}

fn panel_block<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
  Block::default()
    .title(title)
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(Color::Cyan))
}

fn table_header<const N: usize>(labels: [&'static str; N]) -> Row<'static> {
    Row::new(labels).style(
    Style::default()
      .fg(Color::Green)
      .add_modifier(Modifier::BOLD),
  )
}

fn page_title(title: &str, offset: usize, total: usize) -> String {
  if total == 0 {
    return format!("{title} - empty");
  }
  format!(
    "{title} - rows {}-{} of {total}",
    offset + 1,
    (offset + PAGE_STEP).min(total)
  )
}

fn visible_rows(area: Rect) -> usize {
  usize::from(area.height.saturating_sub(4)).max(1)
}

fn percent(value: f64) -> String {
  format!("{value:.1}%")
}

fn label_style() -> Style {
  Style::default()
    .fg(Color::Yellow)
    .add_modifier(Modifier::BOLD)
}

fn format_user(label: &str, user: &triangle::User) -> String {
  format!("{label}: @{} ({})", user.screen_name, user.name)
}

fn user_detail_lines(user: &triangle::User) -> Vec<Line<'static>> {
  vec![
    Line::from(clean_text(if user.name.is_empty() {
      "unknown"
    } else {
      &user.name
    })),
    detail_line("bio", option_text(&user.description)),
    detail_line("verified", verified_text(user.verified)),
    detail_line(
      "id",
      user.id_str.clone().unwrap_or_else(|| "unknown".to_string()),
    ),
    detail_line(
      "social",
      format!(
        "{} followers / {} friends",
        count_text(user.followers_count),
        count_text(user.friends_count)
      ),
    ),
    detail_line("tweets", count_text(user.statuses_count)),
    detail_line("location", option_text(&user.location)),
    detail_line(
      "locale",
      format!(
        "{} / {}",
        option_text(&user.lang),
        option_text(&user.time_zone)
      ),
    ),
    detail_line("joined", option_text(&user.created_at)),
  ]
}

fn detail_line(label: &'static str, value: String) -> Line<'static> {
  Line::from(vec![
    Span::styled(format!("{label}: "), label_style()),
    Span::raw(clean_text(&value)),
  ])
}

fn option_text(value: &Option<String>) -> String {
  value
    .as_deref()
    .filter(|value| !value.trim().is_empty())
    .map(clean_text)
    .unwrap_or_else(|| "unknown".to_string())
}

fn verified_text(value: Option<bool>) -> String {
  match value {
    Some(true) => "yes".to_string(),
    Some(false) => "no".to_string(),
    None => "unknown".to_string(),
  }
}

fn count_text(value: Option<i64>) -> String {
  let Some(value) = value else {
    return "unknown".to_string();
  };

  let abs = value.abs() as f64;
  if abs >= 1_000_000.0 {
    format!("{:.1}M", value as f64 / 1_000_000.0)
  } else if abs >= 1_000.0 {
    format!("{:.1}K", value as f64 / 1_000.0)
  } else {
    value.to_string()
  }
}

fn clean_text(value: &str) -> String {
  value.split_whitespace().collect::<Vec<_>>().join(" ")
}
