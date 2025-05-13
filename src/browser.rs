use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    picking::hover::HoverMap,
    prelude::*,
    winit::WinitSettings,
};
use bevy_simple_text_input::{TextInput, TextInputPlugin, TextInputSettings, TextInputValue};

#[derive(Component)]
struct RequestNode;

#[derive(Component)]
struct BodyNode;

#[derive(Component)]
struct CloseWindow;

#[derive(Resource)]
struct ResponseBody(String);

#[derive(Resource)]
struct RequestUri(String);

const SCROLL_DISTANCE: f32 = 64.;

fn setup(mut commands: Commands, body: Res<ResponseBody>, request: Res<RequestUri>) {
    commands.spawn(Camera2d);

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgb_u8(33, 33, 33)),
            TextColor(Color::srgb_u8(247, 247, 247)),
        ))
        .insert(Pickable::IGNORE)
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::px(16.0, 4.0, 2.0, 2.0),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(Val::Px(1.0)),
                    ..default()
                },
                BorderColor(Color::srgb_u8(48, 50, 52)),
                BackgroundColor(Color::srgb_u8(19, 22, 24)),
            ))
            .with_children(|p| {
                p.spawn(Node {
                    height: Val::Px(35.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|p| {
                    p.spawn(Text("dioscuri | ".to_string()));
                    p.spawn((
                        Node::default(),
                        TextInput,
                        TextInputValue(request.0.trim().to_string()),
                        TextInputSettings {
                            retain_on_submit: true,
                            ..default()
                        },
                        RequestNode,
                    ));
                });

                p.spawn((
                    Button,
                    Node {
                        width: Val::Px(24.0),
                        height: Val::Px(24.0),
                        justify_content: JustifyContent::Center,

                        ..default()
                    },
                ))
                .with_children(|p| {
                    p.spawn(Text("X".to_string()));
                });
            });

            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_self: AlignSelf::Stretch,
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BodyNode,
            ))
            .insert(Pickable {
                should_block_lower: false,
                ..default()
            })
            .with_children(|p| {
                p.spawn(Text::new(body.0.clone())).insert(Pickable {
                    should_block_lower: false,
                    ..default()
                });
            });
        });
}

fn click_close_button(
    mut exit: EventWriter<AppExit>,
    mut interaction_query: Query<&Interaction, (Changed<Interaction>, With<Button>)>,
) {
    for interaction in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
}

fn update_scroll_position(
    mut mouse_wheel_events: EventReader<MouseWheel>,
    mut scrolled_node_query: Query<&mut ScrollPosition>,
    hover_map: Res<HoverMap>,
) {
    for mouse_wheel_event in mouse_wheel_events.read() {
        let (dx, dy) = match mouse_wheel_event.unit {
            MouseScrollUnit::Line => (
                mouse_wheel_event.x * SCROLL_DISTANCE,
                mouse_wheel_event.y * SCROLL_DISTANCE,
            ),
            MouseScrollUnit::Pixel => (mouse_wheel_event.x, mouse_wheel_event.y),
        };

        for (_pointer, pointer_map) in hover_map.iter() {
            for (entity, _hit) in pointer_map.iter() {
                if let Ok(mut scroll_position) = scrolled_node_query.get_mut(*entity) {
                    scroll_position.offset_x -= dx;
                    scroll_position.offset_y -= dy;
                }
            }
        }
    }
}

pub fn run_app(request: String, body: String) {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "dioscuri".to_string(),
                decorations: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(TextInputPlugin)
        .insert_resource(WinitSettings::desktop_app())
        .insert_resource(ResponseBody(body))
        .insert_resource(RequestUri(request))
        .add_systems(Startup, setup)
        .add_systems(Update, (update_scroll_position, click_close_button))
        .run();
}
