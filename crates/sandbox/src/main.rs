mod assemblages;
/// Sandbox
///
/// Development sandbox for antigen functionality
mod components;
mod renderers;
mod resources;
mod systems;
mod threads;

use antigen_cgmath::components::ViewProjectionMatrix;
use antigen_components::{Image, ImageComponent};
use antigen_resources::Timing;
use antigen_wgpu::{WgpuManager, WgpuRequester, WgpuResponder};

use components::*;
use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent};
use resources::*;
use systems::*;

use crossbeam_channel::{Receiver, Sender};
use legion_debugger::{Archetypes, Entities};
use reflection::data::Data;

use antigen_winit::{
    components::RedrawMode, window_manager::WindowManager, WinitRequester, WinitResponder,
};
use reflection_tui::{standard_widgets, DataWidget, ReflectionWidget, ReflectionWidgetState};
use remote_channel::*;
use tui_debugger::Resources as TraceResources;

use legion::*;
use parking_lot::{Mutex, RwLock};
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

/// Calls a function at a set interval until it returns true
///
/// Uses a while loop for timing, which is accurate, but expensive
fn spin_loop(tick_duration: Duration, mut f: impl FnMut() -> bool) -> impl FnMut() {
    move || loop {
        let timestamp = Instant::now();
        if f() {
            break;
        }
        while timestamp.elapsed() < tick_duration {
            std::hint::spin_loop();
        }
    }
}

#[profiling::function]
fn build_world(wgpu_manager: &WgpuManager) -> World {
    log::trace!("Building world");
    let mut world = World::default();

    log::trace!("Populating entities");

    //assemblages::hello_triangle_renderer(&mut world, wgpu_manager);
    assemblages::cube_renderer(&mut world, wgpu_manager);
    //assemblages::msaa_lines_renderer(&mut world, wgpu_manager);
    //assemblages::boids_renderer(&mut world, wgpu_manager);
    //assemblages::conservative_raster_renderer(&mut world, wgpu_manager);
    //assemblages::mipmap_renderer(&mut world, wgpu_manager);
    //assemblages::texture_arrays_renderer(&mut world, wgpu_manager);
    //assemblages::shadow_renderer(&mut world, wgpu_manager);
    //assemblages::bunnymark_renderer(&mut world, wgpu_manager);
    //assemblages::skybox_renderer(&mut world, wgpu_manager);
    //assemblages::water_renderer(&mut world, wgpu_manager);

    // Test entity
    let entity: Entity = world.push((Position { x: 0.0, y: 0.0 }, Velocity { dx: 0.5, dy: 0.0 }));

    let _entities: &[Entity] = world.extend(vec![
        (Position { x: 0.0, y: 0.0 }, Velocity { dx: 1.0, dy: 3.0 }),
        (Position { x: 1.0, y: 1.0 }, Velocity { dx: 2.0, dy: 2.0 }),
        (Position { x: 2.0, y: 2.0 }, Velocity { dx: 3.0, dy: 1.0 }),
    ]);

    if let Some(mut entry) = world.entry(entity) {
        // add an extra component
        entry.add_component(12f32);

        // access the entity's components, returns `None` if the entity does not have the component
        assert_eq!(entry.get_component::<f32>().unwrap(), &12f32);
    }

    // entries return `None` if the entity does not exist
    if let Some(mut entry) = world.entry(entity) {
        // add an extra component
        entry.add_component(12f32);

        // access the entity's components, returns `None` if the entity does not have the component
        assert_eq!(entry.get_component::<f32>().unwrap(), &12f32);
    }

    world
}

/// A type that can be shared across threads and build a set of legion [`Resources`]
///
/// Used to distribute shared resources allocated on the main thread
/// without having to treat them as components
trait SharedState: Send + Sync {
    fn resources(&self) -> legion::Resources;
}

#[derive(Debug, Default, Clone)]
pub struct Shared {
    trace_archetypes: Arc<RwLock<Archetypes>>,
    trace_entities: Arc<RwLock<Entities>>,
    trace_resources: Arc<RwLock<TraceResources>>,
}

impl SharedState for Shared {
    fn resources(&self) -> legion::Resources {
        let mut resources = legion::Resources::default();
        resources.insert(self.trace_archetypes.clone());
        resources.insert(self.trace_entities.clone());
        resources.insert(self.trace_resources.clone());
        resources
    }
}

fn main() {
    profiling::scope!("Main");

    let shared_state = Shared::default();

    let (crossterm_tx, crossterm_rx) = crossbeam_channel::unbounded();

    let window_manager = WindowManager::default();
    let (winit_requester, winit_responder) = remote_channel(window_manager);

    let instance = wgpu::Instance::new(wgpu::Backends::all());

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .unwrap();

    let (device, queue) = pollster::block_on(
        adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: wgpu::Features::POLYGON_MODE_LINE
                | wgpu::Features::CONSERVATIVE_RASTERIZATION
                | wgpu::Features::SPIRV_SHADER_PASSTHROUGH
                | wgpu::Features::PUSH_CONSTANTS
                | wgpu::Features::TEXTURE_BINDING_ARRAY
                //| wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
                | wgpu::Features::UNSIZED_BINDING_ARRAY,
                limits: wgpu::Limits {
                    max_push_constant_size: 4,
                    ..wgpu::Limits::downlevel_defaults()
                }
                .using_resolution(adapter.limits()),
            },
            None,
        ),
    )
    .unwrap();

    let wgpu_manager = WgpuManager::new(instance, adapter, device, queue);
    let queue = wgpu_manager.queue();

    let world = Arc::new(Mutex::new(build_world(&wgpu_manager)));

    let (wgpu_requester, wgpu_responder) = remote_channel(wgpu_manager);

    // Spawn threads
    let main_loop_break = Arc::new(AtomicBool::new(false));

    let game_thread_handle = std::thread::spawn(threads::game_thread(
        world.clone(),
        shared_state.clone(),
        winit_requester,
        wgpu_requester,
        queue,
        main_loop_break.clone(),
    ));

    let tui_input_thread_handle = std::thread::spawn(threads::tui_input_thread(
        crossterm_tx,
        main_loop_break.clone(),
    ));
    let tui_render_thread_handle = std::thread::spawn(threads::tui_render_thread(
        shared_state,
        crossterm_rx,
        main_loop_break.clone(),
    ));

    let join_handles = vec![
        game_thread_handle,
        tui_input_thread_handle,
        tui_render_thread_handle,
    ];

    // Prepare main loop
    let resize_schedule = Schedule::builder()
        .add_system(antigen_wgpu::systems::aspect_ratio_system())
        .add_system(antigen_cgmath::systems::look_at_system())
        .add_system(antigen_cgmath::systems::perspective_projection_system())
        .flush()
        .add_system(antigen_cgmath::systems::view_projection_matrix_system())
        .add_system(antigen_wgpu::systems::buffer_write_system::<
            ViewProjectionMatrix,
            antigen_cgmath::cgmath::Matrix4<f32>,
        >())
        .build();

    // Run main loop
    winit::event_loop::EventLoop::new().run(threads::winit_thread(
        world,
        winit_responder,
        wgpu_responder,
        Some(resize_schedule),
        None,
        main_loop_break,
        join_handles,
    ));
}
