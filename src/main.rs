mod country_with_most_tweets;
mod db_loader;
mod replies_by_blcklcfr;
mod triangle;
mod tweets_per_hashtag;
mod user_tweet_dist;
mod user_with_most_tweets;

use std::{
  error::Error,
  io::Stdout,
  time::{Duration, Instant},
};

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

static CONN_STRING: &str = include_str!("../.mongostr");
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
  started_at: Instant,
  data: QueryData,
}

impl App {
  fn new(data: QueryData) -> Self {
    Self {
      selected_query: 0,
      offset: 0,
      triangle_index: 0,
      started_at: Instant::now(),
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
  let conn_string = CONN_STRING.trim();
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

    if !event::poll(Duration::from_millis(50))? {
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
    "3D Triangles".to_string()
  } else {
    format!(
      "3D Triangles {}/{}",
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

  if inner.width < 56 || inner.height < 18 {
    render_triangle_text_fallback(frame, inner, triple);
    return;
  }

  let sections = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Min(12), Constraint::Length(6)])
    .split(inner);

  render_triangle_scene(
    frame,
    sections[0],
    app.started_at.elapsed().as_secs_f32(),
    triple,
  );
  render_triangle_legend(frame, sections[1], triple);
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

#[derive(Clone, Copy)]
struct Vec3 {
  x: f32,
  y: f32,
  z: f32,
}

#[derive(Clone, Copy)]
struct ProjectedPoint {
  x: u16,
  y: u16,
  depth: f32,
}

fn render_triangle_scene(
  frame: &mut Frame<'_>,
  area: Rect,
  seconds: f32,
  triple: &triangle::Triple,
) {
  let block = panel_block("Rotating Wireframe");
  let scene = block.inner(area);
  frame.render_widget(block, area);

  if scene.width < 10 || scene.height < 8 {
    return;
  }

  let angle = seconds * 0.9;
  let front = [
    Vec3 {
      x: 0.0,
      y: -1.1,
      z: -0.28,
    },
    Vec3 {
      x: -1.45,
      y: 0.95,
      z: -0.28,
    },
    Vec3 {
      x: 1.45,
      y: 0.95,
      z: -0.28,
    },
  ];
  let back = front.map(|point| Vec3 { z: 0.34, ..point });
  let vertices = [
    ("A", &triple.a, Color::Red, 0.0),
    ("B", &triple.b, Color::Yellow, 1.7),
    ("C", &triple.c, Color::Blue, 3.4),
  ];

  let buf = frame.buffer_mut();
  draw_triangle_prism(buf, scene, front, back, angle);

  for (index, (label, user, color, phase)) in vertices.into_iter().enumerate() {
    draw_vertex_cube(buf, scene, front[index], angle, color, phase);
    if let Some(point) = project_point(rotate_scene(front[index], angle), scene) {
      draw_vertex_label(buf, scene, point, label, user, color);
    }
  }

  draw_text(
    buf,
    scene,
    scene.x.saturating_add(1),
    scene.y,
    "h/l cycles triangles; q quits",
    Style::default().fg(Color::DarkGray),
  );
}

fn render_triangle_legend(frame: &mut Frame<'_>, area: Rect, triple: &triangle::Triple) {
  let rows = [
    ("A", &triple.a, Color::Red),
    ("B", &triple.b, Color::Yellow),
    ("C", &triple.c, Color::Blue),
  ];
  let lines = rows
    .into_iter()
    .map(|(label, user, color)| {
      Line::from(vec![
        Span::styled(
          format!("{label} "),
          Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
          format!("@{}", user.screen_name),
          Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
          " | {} | {} followers | verified: {}",
          clean_text(if user.name.is_empty() {
            "unknown"
          } else {
            &user.name
          }),
          count_text(user.followers_count),
          verified_text(user.verified),
        )),
      ])
    })
    .collect::<Vec<_>>();

  frame.render_widget(
    Paragraph::new(lines)
      .alignment(Alignment::Center)
      .block(panel_block("Vertex Users"))
      .wrap(Wrap { trim: true }),
    area,
  );
}

fn draw_triangle_prism(
  buf: &mut Buffer,
  clip: Rect,
  front: [Vec3; 3],
  back: [Vec3; 3],
  angle: f32,
) {
  let front = front.map(|point| project_point(rotate_scene(point, angle), clip));
  let back = back.map(|point| project_point(rotate_scene(point, angle), clip));
  let edges = [(0, 1), (1, 2), (2, 0)];
  let back_style = Style::default().fg(Color::DarkGray);
  let connector_style = Style::default().fg(Color::Gray);
  let front_style = Style::default()
    .fg(Color::Cyan)
    .add_modifier(Modifier::BOLD);

  for (from, to) in edges {
    draw_projected_line(buf, clip, back[from], back[to], back_style);
  }

  for index in 0..3 {
    draw_projected_line(buf, clip, back[index], front[index], connector_style);
  }

  for (from, to) in edges {
    draw_projected_line(buf, clip, front[from], front[to], front_style);
  }
}

fn draw_vertex_cube(
  buf: &mut Buffer,
  clip: Rect,
  center: Vec3,
  angle: f32,
  color: Color,
  phase: f32,
) {
  let half = 0.18;
  let offsets = [
    Vec3 {
      x: -half,
      y: -half,
      z: -half,
    },
    Vec3 {
      x: half,
      y: -half,
      z: -half,
    },
    Vec3 {
      x: half,
      y: half,
      z: -half,
    },
    Vec3 {
      x: -half,
      y: half,
      z: -half,
    },
    Vec3 {
      x: -half,
      y: -half,
      z: half,
    },
    Vec3 {
      x: half,
      y: -half,
      z: half,
    },
    Vec3 {
      x: half,
      y: half,
      z: half,
    },
    Vec3 {
      x: -half,
      y: half,
      z: half,
    },
  ];
  let points = offsets.map(|offset| {
    let spun = rotate_xyz(
      offset,
      angle * 1.8 + phase,
      angle * 1.3 + phase,
      angle * 1.5,
    );
    project_point(rotate_scene(add_vec3(center, spun), angle), clip)
  });
  let edges = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
  ];
  let close_style = Style::default().fg(color).add_modifier(Modifier::BOLD);
  let far_style = Style::default().fg(Color::DarkGray);

  for (from, to) in edges {
    let style = match (points[from], points[to]) {
      (Some(from), Some(to)) if (from.depth + to.depth) / 2.0 > 0.25 => far_style,
      _ => close_style,
    };
    draw_projected_line(buf, clip, points[from], points[to], style);
  }

  for point in points.into_iter().flatten() {
    let style = if point.depth > 0.25 {
      far_style
    } else {
      close_style
    };
    draw_symbol(buf, clip, point.x, point.y, "■", style);
  }
}

fn draw_projected_line(
  buf: &mut Buffer,
  clip: Rect,
  from: Option<ProjectedPoint>,
  to: Option<ProjectedPoint>,
  style: Style,
) {
  if let (Some(from), Some(to)) = (from, to) {
    draw_line(buf, clip, (from.x, from.y), (to.x, to.y), style);
  }
}

fn draw_vertex_label(
  buf: &mut Buffer,
  clip: Rect,
  point: ProjectedPoint,
  label: &str,
  user: &triangle::User,
  color: Color,
) {
  let text = format!("{label} @{}", user.screen_name);
  let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
  let right_edge = clip.x.saturating_add(clip.width);
  let x = if point.x.saturating_add(width).saturating_add(2) < right_edge {
    point.x.saturating_add(2)
  } else {
    point.x.saturating_sub(width.saturating_add(2))
  };
  let y = if point.y > clip.y.saturating_add(1) {
    point.y - 1
  } else {
    point.y.saturating_add(1)
  };

  draw_text(
    buf,
    clip,
    x,
    y,
    &text,
    Style::default().fg(color).add_modifier(Modifier::BOLD),
  );
}

fn project_point(point: Vec3, area: Rect) -> Option<ProjectedPoint> {
  let distance = 4.4;
  let depth = distance + point.z;
  if depth <= 0.1 || area.width == 0 || area.height == 0 {
    return None;
  }

  let perspective = distance / depth;
  let center_x = f32::from(area.x) + f32::from(area.width) / 2.0;
  let center_y = f32::from(area.y) + f32::from(area.height) / 2.0;
  let x = center_x + point.x * f32::from(area.width) / 4.7 * perspective;
  let y = center_y + point.y * f32::from(area.height) / 3.0 * perspective;

  if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
    return None;
  }

  Some(ProjectedPoint {
    x: x.round().min(f32::from(u16::MAX)) as u16,
    y: y.round().min(f32::from(u16::MAX)) as u16,
    depth: point.z,
  })
}

fn rotate_scene(point: Vec3, angle: f32) -> Vec3 {
  rotate_xyz(point, angle * 0.55 + 0.45, angle * 0.8, angle * 0.25)
}

fn rotate_xyz(point: Vec3, x_angle: f32, y_angle: f32, z_angle: f32) -> Vec3 {
  let (sin_x, cos_x) = x_angle.sin_cos();
  let (sin_y, cos_y) = y_angle.sin_cos();
  let (sin_z, cos_z) = z_angle.sin_cos();

  let point = Vec3 {
    x: point.x * cos_y + point.z * sin_y,
    y: point.y,
    z: -point.x * sin_y + point.z * cos_y,
  };
  let point = Vec3 {
    x: point.x,
    y: point.y * cos_x - point.z * sin_x,
    z: point.y * sin_x + point.z * cos_x,
  };
  Vec3 {
    x: point.x * cos_z - point.y * sin_z,
    y: point.x * sin_z + point.y * cos_z,
    z: point.z,
  }
}

fn add_vec3(left: Vec3, right: Vec3) -> Vec3 {
  Vec3 {
    x: left.x + right.x,
    y: left.y + right.y,
    z: left.z + right.z,
  }
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

fn draw_text(buf: &mut Buffer, clip: Rect, x: u16, y: u16, text: &str, style: Style) {
  for (offset, character) in text.chars().enumerate() {
    let Ok(offset) = u16::try_from(offset) else {
      break;
    };
    let Some(cell_x) = x.checked_add(offset) else {
      break;
    };
    let symbol = character.to_string();
    draw_symbol(buf, clip, cell_x, y, &symbol, style);
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
