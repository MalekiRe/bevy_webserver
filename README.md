# Bevy Webserver

A web server integration for the Bevy game engine that allows you to easily append a webserver to Bevy.
For either creating standalone webapps or appending a webserver to an existing bevy app/game.

## Features

- 🚀 Seamless integration with Bevy ECS
- 🌐 Built on top of Axum for all your webserver needs
- ⚡ Async-first design with full ECS access thanks to bevy_defer

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
bevy_webserver = "0.4.0"
bevy = "0.18.0"
axum = "0.8.1"
```

## Quick Start

Here's a minimal example that sets up a simple "Hello World" web server:

```rust
use bevy::prelude::*;
use bevy_webserver::RouterAppExt;

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .route("/hello_world", axum::routing::get(hello_world))
        .run();
}

async fn hello_world() -> axum::response::Html<String> {
    axum::response::Html("<p>Hello world!</p>".to_string())
}
```

## Usage Guide

### Basic Setup

1. Use the `RouterAppExt` trait to add routes
2. Define your handler functions
3. That's it! Your web server is ready to go

```rust
use bevy::prelude::*;
use bevy_webserver::RouterAppExt;

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        // Add as many routes as you need
        .route("/", axum::routing::get(index))
        .route("/about", axum::routing::get(about))
        .route("/api/data", axum::routing::post(handle_data))
        .run();
}
```

### Accessing Bevy ECS from Handlers

The plugin uses `bevy_defer::AsyncWorld` for accessing Bevy's ECS from your web handlers:

```rust
use bevy_defer::AsyncWorld;

async fn get_player_score(Path(player_id): Path<String>) -> impl IntoResponse {
    let scores = AsyncWorld
        .query::<&Score>()
        .get_mut(|query| {
            let mut scores = vec![];
            for score in query.iter() {
                scores.push(score);
            }
            serde_json::serialize(&scores).unwrap()
        });
    
    Json(scores)
}
```

### Template Integration with Maud

Create dynamic HTML templates using my recommendation, Maud:

```rust
use maud::{html, Markup};

fn base_template(content: Markup) -> Markup {
    html! {
        html {
            head {
                title { "My Bevy Web App" }
            }
            body {
                (content)
            }
        }
    }
}

async fn index() -> axum::response::Html<String> {
    let markup = base_template(html! {
        h1 { "Welcome!" }
        p { "This is a Bevy web application." }
    });
    
    axum::response::Html(markup.into_string())
}
```

### HTMX Integration

The plugin works great with HTMX for dynamic content:

```rust
async fn player_list() -> axum::response::Html<String> {
    let players = AsyncWorld
        .query::<(&Player, &Score)>()
        .get_mut(|query| {
            let mut players = vec![];
            for (player, score) in query.iter() {
                players.push((player.clone(), score.clone()));
            }
            players
        })
        .unwrap();

    let markup = html! {
        div class="player-list" {
            @for (player, score) in players {
                div hx-target="this" hx-swap="outerHTML" {
                    (player.name) " - " (score.value)
                }
            }
        }
    };

    axum::response::Html(markup.into_string())
}
```

## Examples

There is a complete example of a web-based game score tracker in examples/crud_app.rs

This also uses another one of my crates bevy_easy_database which makes it easy to persist data!

## Bevy support table

| bevy | bevy_webserver |
|------|----------------|
| 0.18 | 0.4            |
| 0.17 | 0.3            |
| 0.16 | 0.2            |
| 0.15 | 0.1            |

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

MIT OR Apache-2.0

## Credits

Built with ❤️ for the Bevy community.
Built off the back of Axum and bevy_defer