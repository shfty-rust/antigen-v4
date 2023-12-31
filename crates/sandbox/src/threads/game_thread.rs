use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use antigen_components::{Image, ImageComponent};
use antigen_resources::Timing;
use antigen_wgpu::{DrawIndexedIndirect, WgpuRequester};
use antigen_winit::WinitRequester;
use legion::{Schedule, World};
use parking_lot::Mutex;
use wgpu::Queue;

use crate::{
    renderers::cube::{
        Index, IndexedIndirectComponent, Instance, InstanceComponent,
    },
    spin_loop,
    systems::{
        tui_debugger_parse_archetypes_thread_local, tui_debugger_parse_entities_thread_local,
        tui_debugger_parse_resources_thread_local,
    },
    Shared, SharedState,
};

const DEBUGGER: bool = false;

const GAME_TICK_HZ: f64 = 60.0;
const GAME_TICK_SECS: f64 = 1.0 / GAME_TICK_HZ;

pub fn game_thread<'a>(
    world: Arc<Mutex<World>>,
    shared_state: Shared,
    winit_requester: WinitRequester,
    wgpu_requester: WgpuRequester,
    queue: Arc<Queue>,
    main_loop_break: Arc<AtomicBool>,
) -> impl FnOnce() + 'a {
    move || {
        let mut builder = Schedule::builder();

        builder
            .add_system(antigen_resources::systems::timing_update_system(
                Instant::now(),
            ))
            .flush();

        antigen_winit::systems::systems(&mut builder);
        antigen_wgpu::systems::systems(&mut builder);
        antigen_rapier3d::systems(&mut builder);

        builder
            .flush()
            .add_system(crate::renderers::cube::update_look_system())
            .flush()
            .add_system(antigen_wgpu::systems::aspect_ratio_system())
            .add_system(antigen_cgmath::systems::look_at_quat_system())
            .add_system(antigen_cgmath::systems::perspective_projection_system())
            .flush()
            .add_system(crate::renderers::cube::collect_vertices_system())
            .add_system(crate::renderers::cube::collect_indices_system())
            .add_system(crate::renderers::cube::update_instances_system())
            .add_system(crate::renderers::cube::update_uniforms_system())
            .flush()
            .add_system(crate::renderers::cube::collect_instances_indirects_system())
            .flush()
            .add_system(antigen_wgpu::systems::buffer_write_system::<
                crate::assemblages::MeshVerticesVector3,
                Vec<nalgebra::Vector3<f32>>,
            >())
            .add_system(antigen_wgpu::systems::buffer_write_system::<
                crate::assemblages::MeshNormalsVector3,
                Vec<nalgebra::Vector3<f32>>,
            >())
            .add_system(antigen_wgpu::systems::buffer_write_system::<
                crate::assemblages::MeshUvsVector2,
                Vec<nalgebra::Vector2<f32>>,
            >())
            .add_system(antigen_wgpu::systems::buffer_write_system::<
                crate::assemblages::MeshTextureIdsI32,
                Vec<i32>,
            >())
            .add_system(antigen_wgpu::systems::buffer_write_system::<
                crate::assemblages::MeshTriangleIndicesU16,
                Vec<Index>,
            >())
            .add_system(antigen_wgpu::systems::buffer_write_system::<
                antigen_cgmath::components::Position3d,
                antigen_cgmath::cgmath::Vector3<f32>,
            >())
            .add_system(antigen_wgpu::systems::buffer_write_system::<
                antigen_cgmath::components::Orientation,
                antigen_cgmath::cgmath::Quaternion<f32>,
            >())
            .add_system(antigen_wgpu::systems::buffer_write_system::<
                InstanceComponent,
                Instance,
            >())
            .add_system(antigen_wgpu::systems::buffer_write_system::<
                IndexedIndirectComponent,
                DrawIndexedIndirect,
            >())
            .add_system(antigen_wgpu::systems::texture_write_system::<
                ImageComponent,
                Image,
            >())
            .add_system(antigen_wgpu::systems::buffer_write_system::<
                crate::renderers::cube::UniformsComponent,
                crate::renderers::cube::Uniforms,
            >());

        let mut schedule = builder.build();

        let mut resources = shared_state.resources();
        resources.insert(winit_requester);
        resources.insert(wgpu_requester);
        resources.insert(Timing::default());
        resources.insert(queue);

        spin_loop(Duration::from_secs_f64(GAME_TICK_SECS), move || {
            let mut world = world.lock();

            resources
                .get_mut::<WinitRequester>()
                .unwrap()
                .receive_responses(&mut world);

            resources
                .get_mut::<WgpuRequester>()
                .unwrap()
                .receive_responses(&mut world);

            schedule.execute(&mut world, &mut resources);

            if DEBUGGER {
                tui_debugger_parse_archetypes_thread_local()(&mut world, &mut resources);
                tui_debugger_parse_entities_thread_local()(&mut world, &mut resources);
                tui_debugger_parse_resources_thread_local()(&mut world, &mut resources);
            }

            main_loop_break.load(Ordering::Relaxed)
        })()
    }
}
