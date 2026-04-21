use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::client::*;

use shared::channels::GameChannel;
use shared::components::player::Class;
use shared::messages::RequestSpawnMsg;
use shared::settings;

use crate::systems::input;
use crate::systems::keybindings::ActionBarBindings;
use crate::ui;
use crate::ui::character_select;
use crate::ui::hud::HudPlugin;
use crate::world::WorldPlugin;

/// The client ID assigned to this instance.  Used to skip rendering our own
/// server-authoritative player entity (we have a physics-driven local copy).
#[derive(Resource)]
pub struct LocalClientId(pub u64);

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    CharacterSelect,
    InGame,
}

/// Set when the player clicks a class button.
#[derive(Resource, Default)]
pub struct ClassChosen(pub Option<Class>);

pub struct ClientGamePlugin;

impl Plugin for ClientGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((WorldPlugin, HudPlugin));
        app.init_state::<AppState>();
        app.init_resource::<ActionBarBindings>();
        app.init_resource::<ClassChosen>();

        // Assign a stable client ID immediately so it's available during character select.
        app.add_systems(Startup, assign_client_id);

        // Class selection screen.
        app.add_systems(OnEnter(AppState::CharacterSelect), character_select::spawn_class_select);
        app.add_systems(OnExit(AppState::CharacterSelect),  character_select::despawn_class_select);
        app.add_systems(
            Update,
            character_select::handle_class_select.run_if(in_state(AppState::CharacterSelect)),
        );

        // Connect to server when transitioning into the game.
        // By deferring until InGame, the character select screen is the first thing shown.
        app.add_systems(OnEnter(AppState::InGame), connect_to_server);

        // Send RequestSpawnMsg as soon as the Netcode handshake completes.
        // ClassChosen is always Some by this point since we connect after selection.
        app.add_observer(on_client_connected);

        // Gather local input every frame and send to server — only once in-game.
        app.add_systems(
            Update,
            (
                input::gather_and_send_input,
                input::send_target_selection,
                // DEV-ONLY — REMOVE: keys 4/5/6 apply typed DoTs for testing.
                crate::systems::dev_dots::send_dev_dot_requests,
            )
                .run_if(in_state(AppState::InGame)),
        );

        // Minigame UI rendering (reads replicated server state, draws overlay) — only in-game.
        app.add_systems(
            Update,
            (
                ui::arc::render_arc,
                ui::cube::render_cube,
                ui::bar_fill::render_bar_fill,
                ui::wave_interference::render_wave_interference,
                ui::value_lock::render_value_lock,
                ui::heartbeat::render_heartbeat,
            )
            .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Generates and stores the client ID at startup so it's available during character selection.
fn assign_client_id(mut commands: Commands) {
    let client_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);
    commands.insert_resource(LocalClientId(client_id));
}

/// Fires when the Netcode handshake completes. ClassChosen is always Some here because
/// we only connect after the player has made their selection.
fn on_client_connected(
    trigger: On<Add, Connected>,
    mut sender_query: Query<&mut MessageSender<RequestSpawnMsg>>,
    local_id: Res<LocalClientId>,
    class_chosen: Res<ClassChosen>,
) {
    let Some(class) = class_chosen.0.clone() else {
        warn!("[CLIENT] Connected but no class chosen — this should not happen");
        return;
    };
    let entity = trigger.event_target();
    let Ok(mut sender) = sender_query.get_mut(entity) else { return };
    let name = format!("Player{}", local_id.0 % 1000);
    let subclass = character_select::default_subclass(&class);
    sender.send::<GameChannel>(RequestSpawnMsg { name: name.clone(), class, subclass });
    info!("[CLIENT] Connected — sent RequestSpawnMsg for '{name}'");
}

/// Spawns the client transport entity and initiates the connection to the server.
fn connect_to_server(mut commands: Commands, local_id: Res<LocalClientId>) {
    let auth = Authentication::Manual {
        server_addr: settings::client_connect_addr(),
        client_id: local_id.0,
        private_key: [0u8; 32],
        protocol_id: settings::PROTOCOL_ID,
    };

    let netcode_client = NetcodeClient::new(auth, NetcodeConfig::default())
        .expect("Failed to build netcode client");

    let entity = commands.spawn((
        Name::new("GameClient"),
        netcode_client,
        UdpIo::default(),
        LocalAddr("0.0.0.0:0".parse().unwrap()),
        ReplicationReceiver::default(),
    )).id();
    commands.entity(entity).trigger(|e| Connect { entity: e });
    info!("[CLIENT] Spawned NetcodeClient {entity:?} (id={}), triggering Connect", local_id.0);
}
