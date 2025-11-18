use async_io::Async;
use axum::extract::Path;
use axum::routing::post;
use axum::{
    extract::State,
    response::Html,
    routing::{delete, get, put},
    Json,
};
use bevy::ecs::component::ComponentInfo;
use bevy::ecs::entity::Entities;
use bevy::ecs::world::error::EntityComponentError;
use bevy::reflect::{
    DynamicTypePath, EnumInfo, ReflectFromPtr, StructInfo, TupleStructInfo, TypeInfo, TypeRegistry,
};
use bevy::{color::palettes::tailwind, prelude::*};
use bevy_defer::AsyncWorld;
use bevy_webserver::{BevyWebServerPlugin, RouterAppExt};
use maud::{html, Markup, PreEscaped};
use std::any::Any;
use std::ops::Deref;

pub struct EditorCorePlugin;

impl Plugin for EditorCorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedEntity>()
            .register_type::<SelectedEntity>()
            .add_systems(PostUpdate, reset_selected_entity_if_entity_despawned);
    }
}

/// The currently selected entity in the scene.
#[derive(Resource, Default, Reflect)]
#[reflect(Resource, Default)]
pub struct SelectedEntity(pub Option<Entity>);

/// System to reset [`SelectedEntity`] when the entity is despawned.
pub fn reset_selected_entity_if_entity_despawned(
    mut selected_entity: ResMut<SelectedEntity>,
    entities: &Entities,
) {
    if let Some(e) = selected_entity.0 {
        if !entities.contains(e) {
            selected_entity.0 = None;
        }
    }
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, WebInspectorPlugin))
        .insert_resource(SelectedEntity::default())
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn((Camera2d));
    commands.spawn((Name::new("owo"), Transform::default()));
}

pub struct WebInspectorPlugin;

impl Plugin for WebInspectorPlugin {
    fn build(&self, app: &mut App) {
        app.route("/", get(render_layout))
            .route("/inspector", get(render_inspector))
            .route(
                "/component/{entity}/{component}/{field-name}",
                put(update_component_field),
            )
            .route("/component/{entity}", delete(delete_component))
            .route("/entities", get(render_entity_list))
            .route("/entities/select/{entity}", post(select_entity));
    }
}

#[derive(serde::Deserialize)]
struct FieldUpdate {
    value: serde_json::Value,
}

async fn update_component_field(
    Path((entity_index, component_name, field_name)): Path<(u32, String, String)>,
    Json(update): Json<FieldUpdate>,
) -> Html<String> {
    AsyncWorld.run(|world| {
        if let Some(entity) = Entity::from_raw_u32(entity_index) {
            // Get type registry for reflection
            let type_registry = world.resource::<AppTypeRegistry>().clone();
            let type_registry = type_registry.read();

            let mut stuff = None;

            for component in world.components().iter_registered() {
                if let Some(info) = type_registry.get_type_info(component.type_id().unwrap()) {
                    let component_short_name = info.type_path().split("::").last().unwrap_or("");

                    if component_short_name == component_name {
                        stuff.replace((info.clone(), component.type_id().unwrap(), component.id()));
                    }
                }
            }
            if let Some((info, type_id, id)) = stuff {
                if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                    // Get mutable reference to component
                    if let Ok(mut component_ref) = entity_mut.get_mut_by_id(id) {
                        let reflect_data = type_registry.get(type_id).unwrap();
                        let reflect_from_ptr = reflect_data.data::<ReflectFromPtr>().unwrap();
                        // SAFE: `value` is of type `Reflected`, which the `ReflectFromPtr` was created for
                        let value = unsafe { reflect_from_ptr.as_reflect_mut(component_ref.as_mut()) };
                        if let Ok(mut struct_info) = value.reflect_mut().as_struct() {
                            // Find the field and update it

                            let field = struct_info.field_mut(&field_name).unwrap();

                            let field_type_name = field
                                .try_as_reflect()
                                .unwrap()
                                .reflect_type_ident()
                                .unwrap();

                            // Handle different field types
                            match field_type_name {
                                "Vec3" => {
                                    if let Ok(vec3_value) =
                                        serde_json::from_value::<[f32; 3]>(update.value)
                                    {
                                        let vec3 =
                                            Vec3::new(vec3_value[0], vec3_value[1], vec3_value[2]);
                                        field.apply(&vec3);
                                    }
                                }
                                "f32" => {
                                    if let Ok(float_value) = serde_json::from_value::<f32>(update.value)
                                    {
                                        field.apply(&float_value);
                                    }
                                }
                                "String" => {
                                    if let Ok(string_value) =
                                        serde_json::from_value::<String>(update.value)
                                    {
                                        field.apply(&string_value);
                                    }
                                }
                                "bool" => {
                                    if let Ok(bool_value) = serde_json::from_value::<bool>(update.value)
                                    {
                                        field.apply(&bool_value);
                                    }
                                }
                                "Color" => {
                                    if let Ok(color_value) =
                                        serde_json::from_value::<[f32; 4]>(update.value)
                                    {
                                        let color = Color::srgba(
                                            color_value[0],
                                            color_value[1],
                                            color_value[2],
                                            color_value[3],
                                        );
                                        field.apply(&color);
                                    }
                                }
                                "Quat" => {
                                    if let Ok(quat_value) =
                                        serde_json::from_value::<[f32; 4]>(update.value)
                                    {
                                        let quat = Quat::from_xyzw(
                                            quat_value[0],
                                            quat_value[1],
                                            quat_value[2],
                                            quat_value[3],
                                        );
                                        field.apply(&quat);
                                    }
                                }
                                // Add more type handlers as needed
                                _ => {
                                    /*// Try to deserialize directly if type implements FromReflect
                                    if let Ok(value) =
                                        serde_json::from_value(update.value.clone())
                                    {
                                        field.apply(&value);
                                    }*/
                                }
                            }
                            return Html("Field updated successfully".to_string());
                        }
                    }
                }
            }
        }

        Html("Failed to update field".to_string())
    })
}

// State structure to hold component values
#[derive(serde::Serialize, serde::Deserialize)]
struct ComponentValue {
    value: serde_json::Value,
}

// ... [Previous plugin and struct definitions remain the same until render_layout]

async fn render_layout() -> Html<String> {
    Html(
        html! {
            html {
                head {
                    title { "Bevy Web Inspector" }
                    script src="https://cdnjs.cloudflare.com/ajax/libs/htmx/1.9.10/htmx.min.js" {}
                    script src="https://cdn.jsdelivr.net/gh/Emtyloc/json-enc-custom@main/json-enc-custom.js" {}
                    link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/bootstrap/5.3.2/css/bootstrap.min.css" {}
                    script src="https://cdnjs.cloudflare.com/ajax/libs/bootstrap/5.3.2/js/bootstrap.bundle.min.js" {}
                    style { (INSPECTOR_STYLES) }
                }
                body class="bg-dark" {
                    div class="container-fluid vh-100 p-0" {
                        div class="row h-100 g-0" {
                            // Entity list panel
                            div class="col-3 border-end border-secondary"
                                hx-get="/entities"
                                hx-trigger="load"
                                hx-swap="innerHTML" {}
                            // Inspector panel
                            div id="inspector"
                                class="col-9"
                                hx-get="/inspector"
                                hx-trigger="load"
                                hx-swap="innerHTML" {}
                        }
                    }
                }
            }
        }
            .into_string(),
    )
}

async fn render_entity_list() -> Html<String> {
    AsyncWorld.run(|world| -> Html<String> {
        let selected = world.resource::<SelectedEntity>().0;
        let markup = html! {
            div class="entity-list p-3 bg-dark" {
                h2 class="h4 text-light mb-4" { "Entities" }

                div class="entity-cards" {
                    @for (entity, name) in get_named_entities(world) {
                        form class="card bg-secondary mb-3"
                             hx-post=(format!("/entities/select/{}", entity.index()))
                             hx-target="#inspector"
                             hx-swap="innerHTML" {

                            div class="card-body" {
                                // Entity info section
                                div class="d-flex justify-content-between align-items-center mb-2" {
                                    div {
                                        span class="badge bg-dark text-light" {
                                            "#" (entity.index())
                                        }
                                        @if let Some(name) = &name {
                                            span class="ms-2 text-light" {
                                                (name)
                                            }
                                        }
                                    }

                                    span class="badge bg-info" {
                                        (get_component_count(world, entity)) " components"
                                    }
                                }

                                button type="submit"
                                        class=(format!("btn btn-sm w-100 {}",
                                            if Some(entity) == selected {
                                                "btn-success"
                                            } else {
                                                "btn-outline-light"
                                            }
                                        )) {
                                    @if Some(entity) == selected {
                                        "Selected"
                                    } @else {
                                        "Select"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };
        Html(markup.into_string())
    })
}

fn render_component(
    component_info: ComponentInfo,
    type_registry: &TypeRegistry,
    entity: Entity,
    world: &World, // Add world parameter
    component_name: &str,
) -> Markup {
    let binding = component_info.name();
    let (_, name) = binding.rsplit_once("::").unwrap();
    let type_info = component_info
        .type_id()
        .and_then(|type_id| type_registry.get_type_info(type_id));

    // Get the actual component data
    let component_data = if let Some(type_id) = component_info.type_id() {
        match world
            .entity(entity)
            .get_by_id(component_info.id())
            .map(|component| {
                let reflect_data = type_registry.get(type_id)?;
                let reflect_from_ptr = reflect_data.data::<ReflectFromPtr>()?;
                Some(unsafe { reflect_from_ptr.as_reflect(component) })
            }) {
            Ok(Some(awa)) => Some(awa),
            _ => None,
        }
    } else {
        return html! {};
    };

    html! {
        div class="card bg-secondary mb-3" {
            div class="card-header" {
                h4 class="card-title h6 mb-0 text-light" { (name) }
            }
            div class="card-body" {
                @if let (Some(type_info), Some(component_data)) = (type_info, component_data) {
                    (render_type_info(type_info, entity, name, component_data))
                } @else {
                    p class="text-light small mb-0" { "Reflect not implemented" }
                }
            }
        }
    }
}

fn render_component_list(entity: Entity, world: &World) -> Markup {
    let type_registry = world.resource::<AppTypeRegistry>().read();

    html! {
        div class="component-list p-3" {
            h3 class="h5 text-light mb-3" { "Entity Components" }
            @for component_info in world.inspect_entity(entity).unwrap() {
                (render_component(
                    component_info.clone(),
                    &type_registry,
                    entity,
                    world,  // Pass world to render_component
                    &component_info.name()
                ))
            }
        }
    }
}

fn render_struct(
    struct_info: &StructInfo,
    entity: Entity,
    component_name: &str,
    component_data: &dyn Reflect,
) -> Markup {
    let struct_data = component_data.reflect_ref().as_struct().unwrap();

    html! {
        div class="struct-fields card bg-secondary" {
            div class="card-body" {
                @for field in struct_info.iter() {
                    div class="mb-3" {
                        label class="form-label text-light small" { (field.name()) }
                        @if field.type_path_table().short_path() == "glam::Vec3" {
                            @let vec3 = struct_data.field(field.name()).unwrap().try_downcast_ref::<Vec3>().unwrap();
                            div class="row g-2" {
                                @for (axis, value) in [("x", vec3.x), ("y", vec3.y), ("z", vec3.z)] {
                                    div class="col" {
                                        input type="number"
                                            class="form-control form-control-sm bg-dark text-light border-secondary"
                                            name=(axis)
                                            value=(value)
                                            step="0.1"
                                            hx-put={"/component/" (entity.index()) "/" (component_name) "/" (field.name())}
                                            hx-headers=(PreEscaped(r#"{"Content-Type": "application/json"}"#))
                                            parse-types="true"
                                            hx-ext="json-enc-custom"
                                            hx-trigger="change" {}
                                    }
                                }
                            }
                        } @else {
                            @let field_value = struct_data.field(field.name()).unwrap();
                            input type="text"
                                class="form-control form-control-sm bg-dark text-light border-secondary"
                                name=(field.name())
                                value=(format!("{:?}", field_value))
                                hx-put={"/component/" (entity.index()) "/" (component_name) "/" (field.name())}
                                hx-headers=(PreEscaped(r#"{"Content-Type": "application/json"}"#))
                                parse-types="true"
                                hx-ext="json-enc-custom"
                                hx-trigger="change" {}
                        }
                    }
                }
            }
        }
    }
}

fn render_enum(enum_info: &EnumInfo) -> Markup {
    html! {
        div class="enum-variants" {
            select class="form-select form-select-sm bg-dark text-light border-secondary"
                   hx-put="/component/variant"
                   hx-headers=(PreEscaped(r#"{"Content-Type": "application/json"}"#))
                   parse-types="true"
                   hx-ext="json-enc-custom"
                   hx-trigger="change" {
                @for variant in enum_info.iter() {
                    option value=(variant.name()) { (variant.name()) }
                }
            }
        }
    }
}

fn render_vec3_input(entity: Entity, field_name: &str, value: Vec3) -> Markup {
    html! {
        div class="vector-input mb-3" {
            label class="form-label text-light small" { (field_name) }
            div class="row g-2" {
                @for (component, val) in [("x", value.x), ("y", value.y), ("z", value.z)] {
                    div class="col" {
                        input type="number"
                              class="form-control form-control-sm bg-dark text-light border-secondary"
                              name=(component)
                              value=(val)
                              step="10"
                              hx-put={"/transform/" (serde_json::to_string(&entity).unwrap()) "/" (field_name)}
                              hx-headers=(PreEscaped(r#"{"Content-Type": "application/json"}"#))
                              parse-types="true"
                              hx-ext="json-enc-custom"
                              hx-trigger="change" {}
                    }
                }
            }
        }
    }
}

// Helper function to get entities with their names
fn get_named_entities(world: &mut World) -> Vec<(Entity, Option<String>)> {
    let mut entities = Vec::new();

    // Get the type registry to inspect components
    let type_registry = world.resource::<AppTypeRegistry>().clone();

    // Query for all entities that optionally have a Name component
    let mut query = world.query::<(Entity, Option<&Name>)>();
    for (entity, name) in query.iter(world) {
        let name = name.map(|name| name.as_str().to_string());

        // Only include entities that have at least one reflected component
        if world
            .inspect_entity(entity)
            .unwrap()
            .filter(|info| {
                let type_register = type_registry.clone();
                let type_register = type_register.read();
                type_register
                    .get_type_info(info.clone().type_id().unwrap())
                    .is_some()
            })
            .next()
            .is_some()
        {
            entities.push((entity, name));
        }
    }

    // Sort first by presence of name, then by entity ID for stable ordering
    entities.sort_by(|(entity_a, name_a), (entity_b, name_b)| {
        name_a
            .is_some()
            .cmp(&name_b.is_some())
            .reverse()
            .then_with(|| entity_a.index().cmp(&entity_b.index()))
    });

    entities
}

// Helper function to count components on an entity
fn get_component_count(world: &World, entity: Entity) -> usize {
    let type_registry = world.resource::<AppTypeRegistry>();
    let type_registry = type_registry.clone();

    // Only count reflected components
    world
        .inspect_entity(entity)
        .unwrap()
        .filter(move |info| {
            let type_registry = type_registry.clone();
            let type_registry = type_registry.read();
            type_registry
                .get_type_info(info.clone().type_id().unwrap())
                .is_some()
        })
        .count()
}

async fn select_entity(
    axum::extract::Path(entity_index): axum::extract::Path<u32>,
) -> Html<String> {
    AsyncWorld.run(|world| {
        // Create entity from index and update selected entity
        if let Some(entity) = Entity::from_raw_u32(entity_index) {
            if world.get_entity(entity).is_ok() {
                world.resource_mut::<SelectedEntity>().0 = Some(entity);
            }
        }
    });
    // Return the updated inspector content
    let markup = render_inspector().await;
    markup
}

async fn render_inspector() -> Html<String> {
    AsyncWorld.run(|world| -> Html<String> {
        let markup = html! {
            div class="inspector-container" {
                @if let Some(selected_entity) = world.resource::<SelectedEntity>().0 {
                    (render_component_list(selected_entity, &world))
                } @else {
                    p class="text-neutral-300 text-sm" { "Select an entity to inspect" }
                }
            }
        };

        Html(markup.into_string())
    })
}

fn render_type_info(
    type_info: &TypeInfo,
    entity: Entity,
    component_name: &str,
    component_data: &dyn Reflect,
) -> Markup {
    match type_info {
        TypeInfo::Struct(info) => render_struct(info, entity, component_name, component_data),
        TypeInfo::TupleStruct(info) => render_tuple_struct(info),
        TypeInfo::Enum(info) => render_enum(info),
        _ => html! { p { "Type not yet supported" } },
    }
}

fn render_tuple_struct(tuple_struct_info: &TupleStructInfo) -> Markup {
    html! {
        div class="tuple-struct-fields" {
            @for (idx, field) in tuple_struct_info.iter().enumerate() {
                div class="field-row" {
                    label class="text-xs" { (idx) }
                    input type="text"
                          name=(idx.to_string())
                          value=""
                          hx-put={"/component/" (idx)}
                          hx-headers=(PreEscaped(r#"{"Content-Type": "application/json"}"#))
                          parse-types="true"
                          hx-ext="json-enc-custom"
                          hx-trigger="change" {}
                }
            }
        }
    }
}

async fn update_component(
    axum::extract::Path((entity, component)): axum::extract::Path<(Entity, String)>,
    Json(value): Json<ComponentValue>,
) -> Html<String> {
    // Update component logic here
    // Return updated component markup
    Html("Updated".to_string())
}

async fn delete_component(
    axum::extract::Path(entity): axum::extract::Path<Entity>,
) -> Html<String> {
    // Delete component logic here
    Html("".to_string())
}

// CSS styles for the inspector
const INSPECTOR_STYLES: &str = r#"
.inspector-container {
    padding: 1rem;
    background-color: rgb(82 82 91);
    height: 100%;
    overflow-y: auto;
}

.component-card {
    background-color: rgb(63 63 70);
    padding: 0.75rem;
    border-radius: 0.375rem;
    margin-bottom: 0.5rem;
}

.field-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.25rem;
}

.vector-input {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.25rem;
}

input[type="number"],
input[type="text"],
select {
    background-color: rgb(39 39 42);
    color: white;
    border: 1px solid rgb(82 82 91);
    border-radius: 0.25rem;
    padding: 0.25rem 0.5rem;
    font-size: 0.875rem;
    width: 100%;
}

.vector-label {
    grid-column: span 3;
    font-size: 0.75rem;
    color: rgb(212 212 216);
}
"#;
