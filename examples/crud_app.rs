use bevy::prelude::*;
use bevy_defer::AsyncWorld;
use bevy_easy_database::*;
use bevy_webserver::RouterAppExt;
use maud::{html, Markup, DOCTYPE};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct Player(pub String);

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct Score(pub u32);

fn main() {
    App::new()
        .add_plugins((
            MinimalPlugins,
            bevy_webserver::BevyWebServerPlugin,
            DatabasePlugin,
        ))
        .add_database_mapping::<Player>()
        .add_database_mapping::<Score>()
        // Routes
        .route("/", axum::routing::get(index))
        .route("/players", axum::routing::get(list_players))
        .route("/players/new", axum::routing::post(create_player))
        .route("/players/{id}/update", axum::routing::post(update_score))
        .route("/players/{id}/delete", axum::routing::delete(delete_player))
        .run();
}

// Template for the base layout
fn base_template(content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                title { "Game Score Tracker" }
                script src="https://unpkg.com/htmx.org@1.9.10" {}
                link href="https://fonts.googleapis.com/css2?family=Poppins:wght@400;600&display=swap" rel="stylesheet" {}
                style {
                    (CSS)
                }
            }
            body {
                h1 { "Game Score Tracker" }
                (content)
            }
        }
    }
}

// Index page with form to add new players
async fn index() -> axum::response::Html<String> {
    let markup = base_template(html! {
        div {
            form hx-post="/players/new" hx-target="#player-list" hx-swap="outerHtml" {
                label for="name" { "Player Name: " }
                input type="text" name="name" required;
                button type="submit" { "Add Player" }
            }

            div id="player-list" hx-get="/players" hx-trigger="load" {}
        }
    });

    axum::response::Html(markup.into_string())
}

// List all players
async fn list_players() -> axum::response::Html<String> {
    let mut query = AsyncWorld.query::<(&Player, &Score)>();
    let players = query
        .get_mut(|mut query| -> Vec<(Player, Score)> {
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
                (update_from_player(&player, &score))
            }
        }
    };

    axum::response::Html(markup.into_string())
}

// Create a new player
async fn create_player(form: axum::Form<PlayerForm>) -> axum::response::Html<String> {
    AsyncWorld.spawn_bundle((Player(form.name.clone()), Score(0)));
    // yielding so the time we come back we'll have the player spawned in
    AsyncWorld.yield_now().await;
    list_players().await
    // Return updated player list
}

fn update_from_player(player: &Player, score: &Score) -> Markup {
    html! {
        div class="player-item" {
        span { (player.0) " - Score: " (score.0) }

        form hx-swap="outerHTML" hx-target="closest .player-item" hx-post={"/players/" (player.0) "/update"} hx-trigger="change" style="display: inline;" {
            input type="number" name="score" value=(score.0);
        }

        button
            hx-delete={"/players/" (player.0) "/delete"}
            hx-target="closest .player-item"
            hx-swap="outerHTML"
            { "Delete" }
        }
    }
}

// Update player's score
async fn update_score(
    path: axum::extract::Path<String>,
    form: axum::Form<ScoreForm>,
) -> axum::response::Html<String> {
    AsyncWorld.run(|world| -> axum::response::Html<String> {
        let player_name = path.0;

        let mut query = world.query::<(&Player, &mut Score)>();
        for (player, mut score) in query.iter_mut(world) {
            if player.0 == player_name {
                if let Ok(form_score) = u32::from_str(&form.score) {
                    score.0 = form_score;
                }
                return axum::response::Html(update_from_player(player, &score).into_string());
            }
        }
        return axum::response::Html("".to_string());
    })
}

// Delete a player
async fn delete_player(path: axum::extract::Path<String>) -> axum::response::Html<String> {
    AsyncWorld.apply_command(|world: &mut World| {
        let player_name = path.0;

        let mut query = world.query::<(Entity, &Player)>();
        for (entity, player) in query.iter(&world) {
            if player.0 == player_name {
                world.despawn(entity);
                break;
            }
        }
    });
    axum::response::Html("".to_string())
}

#[derive(Deserialize)]
struct PlayerForm {
    name: String,
}

#[derive(Deserialize)]
struct ScoreForm {
    score: String,
}

const CSS: &'static str = r#"
                    :root {
                        --primary-color: #4f46e5;
                        --primary-hover: #4338ca;
                        --background: #f3f4f6;
                        --card-bg: #ffffff;
                        --text: #1f2937;
                        --text-light: #6b7280;
                        --danger: #ef4444;
                        --danger-hover: #dc2626;
                        --success: #10b981;
                    }

                    * {
                        margin: 0;
                        padding: 0;
                        box-sizing: border-box;
                    }

                    body {
                        font-family: 'Poppins', sans-serif;
                        background-color: var(--background);
                        color: var(--text);
                        line-height: 1.5;
                        min-height: 100vh;
                    }

                    .container {
                        max-width: 800px;
                        margin: 0 auto;
                        padding: 2rem;
                    }

                    h1 {
                        font-size: 2.5rem;
                        font-weight: 600;
                        color: var(--primary-color);
                        margin-bottom: 2rem;
                        text-align: center;
                    }

                    .add-player-form {
                        background-color: var(--card-bg);
                        padding: 2rem;
                        border-radius: 1rem;
                        box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1);
                        margin-bottom: 2rem;
                        margin-top: 2rem;
                    }

                    .form-group {
                        display: flex;
                        gap: 1rem;
                        align-items: center;
                    }

                    label {
                        font-weight: 600;
                        min-width: 120px;
                    }

                    input[type="text"],
                    input[type="number"] {
                        flex: 1;
                        padding: 0.75rem 1rem;
                        border: 2px solid #e5e7eb;
                        border-radius: 0.5rem;
                        font-size: 1rem;
                        transition: border-color 0.2s;
                    }

                    input[type="text"]:focus,
                    input[type="number"]:focus {
                        outline: none;
                        border-color: var(--primary-color);
                        box-shadow: 0 0 0 3px rgba(79, 70, 229, 0.1);
                    }

                    button {
                        background-color: var(--primary-color);
                        color: white;
                        padding: 0.75rem 1.5rem;
                        border: none;
                        border-radius: 0.5rem;
                        font-weight: 600;
                        cursor: pointer;
                        transition: background-color 0.2s;
                    }

                    button:hover {
                        background-color: var(--primary-hover);
                    }

                    .player-list {
                        display: grid;
                        gap: 1rem;
                    }

                    .player-item {
                        background-color: var(--card-bg);
                        padding: 1.5rem;
                        border-radius: 0.75rem;
                        box-shadow: 0 2px 4px rgba(0, 0, 0, 0.05);
                        display: flex;
                        align-items: center;
                        gap: 1rem;
                        transition: transform 0.2s;
                    }

                    .player-item:hover {
                        transform: translateY(-2px);
                    }

                    .player-info {
                        flex: 1;
                        font-size: 1.1rem;
                    }

                    .score-input {
                        width: 100px;
                    }

                    .delete-btn {
                        background-color: var(--danger);
                        padding: 0.5rem 1rem;
                    }

                    .delete-btn:hover {
                        background-color: var(--danger-hover);
                    }
                    "#;
