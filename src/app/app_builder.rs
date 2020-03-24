use crate::{
    app::{system_stage, App},
    asset::*,
    core::Time,
    legion::prelude::{Resources, Runnable, Schedulable, Schedule, Universe, World},
    plugin::load_plugin,
    prelude::StandardMaterial,
    render::{
        draw_target::draw_targets::*, mesh::Mesh, pass::passes::*, pipeline::pipelines::*,
        render_resource::resource_providers::*, renderer::Renderer, texture::Texture, *,
    },
    ui,
};

use bevy_transform::{prelude::LocalToWorld, transform_system_bundle};
use pipeline::PipelineDescriptor;
use render_graph::{RenderGraph, RenderGraphBuilder};
use render_resource::{
    build_entity_render_resource_assignments_system, AssetBatchers,
    EntityRenderResourceAssignments, RenderResourceAssignmentsProvider,
};
use shader::Shader;
use std::collections::HashMap;

pub struct AppBuilder {
    pub world: Option<World>,
    pub resources: Option<Resources>,
    pub universe: Option<Universe>,
    pub renderer: Option<Box<dyn Renderer>>,
    pub render_graph: Option<RenderGraph>,
    pub setup_systems: Vec<Box<dyn Schedulable>>,
    pub system_stages: HashMap<String, Vec<Box<dyn Schedulable>>>,
    pub runnable_stages: HashMap<String, Vec<Box<dyn Runnable>>>,
    pub stage_order: Vec<String>,
}

impl AppBuilder {
    pub fn new() -> Self {
        let universe = Universe::new();
        let world = universe.create_world();
        let resources = Resources::default();
        AppBuilder {
            universe: Some(universe),
            world: Some(world),
            resources: Some(resources),
            render_graph: Some(RenderGraph::default()),
            renderer: None,
            setup_systems: Vec::new(),
            system_stages: HashMap::new(),
            runnable_stages: HashMap::new(),
            stage_order: Vec::new(),
        }
    }

    pub fn build(&mut self) -> App {
        let mut setup_schedule_builder = Schedule::builder();
        for setup_system in self.setup_systems.drain(..) {
            setup_schedule_builder = setup_schedule_builder.add_system(setup_system);
        }

        let mut setup_schedule = setup_schedule_builder.build();
        setup_schedule.execute(
            self.world.as_mut().unwrap(),
            self.resources.as_mut().unwrap(),
        );

        let mut schedule_builder = Schedule::builder();
        for stage_name in self.stage_order.iter() {
            if let Some((_name, stage_systems)) = self.system_stages.remove_entry(stage_name) {
                for system in stage_systems {
                    schedule_builder = schedule_builder.add_system(system);
                }

                schedule_builder = schedule_builder.flush();
            }

            if let Some((_name, stage_runnables)) = self.runnable_stages.remove_entry(stage_name) {
                for system in stage_runnables {
                    schedule_builder = schedule_builder.add_thread_local(system);
                }

                schedule_builder = schedule_builder.flush();
            }
        }

        self.resources
            .as_mut()
            .unwrap()
            .insert(self.render_graph.take().unwrap());

        App::new(
            self.universe.take().unwrap(),
            self.world.take().unwrap(),
            schedule_builder.build(),
            self.resources.take().unwrap(),
            self.renderer.take(),
        )
    }

    pub fn run(&mut self) {
        self.build().run();
    }

    pub fn with_world(&mut self, world: World) -> &mut Self {
        self.world = Some(world);
        self
    }

    pub fn setup_world(&mut self, setup: impl Fn(&mut World, &mut Resources)) -> &mut Self {
        setup(
            self.world.as_mut().unwrap(),
            self.resources.as_mut().unwrap(),
        );
        self
    }

    pub fn add_system(&mut self, system: Box<dyn Schedulable>) -> &mut Self {
        self.add_system_to_stage(system_stage::UPDATE, system)
    }

    pub fn add_setup_system(&mut self, system: Box<dyn Schedulable>) -> &mut Self {
        self.setup_systems.push(system);
        self
    }

    pub fn add_system_to_stage(
        &mut self,
        stage_name: &str,
        system: Box<dyn Schedulable>,
    ) -> &mut Self {
        if let None = self.system_stages.get(stage_name) {
            self.system_stages
                .insert(stage_name.to_string(), Vec::new());
            self.stage_order.push(stage_name.to_string());
        }

        let stages = self.system_stages.get_mut(stage_name).unwrap();
        stages.push(system);

        self
    }

    pub fn add_runnable_to_stage(
        &mut self,
        stage_name: &str,
        system: Box<dyn Runnable>,
    ) -> &mut Self {
        if let None = self.runnable_stages.get(stage_name) {
            self.runnable_stages
                .insert(stage_name.to_string(), Vec::new());
            self.stage_order.push(stage_name.to_string());
        }

        let stages = self.runnable_stages.get_mut(stage_name).unwrap();
        stages.push(system);

        self
    }

    pub fn add_default_resources(&mut self) -> &mut Self {
        let mut asset_batchers = AssetBatchers::default();
        asset_batchers.batch_types2::<Mesh, StandardMaterial>();
        let resources = self.resources.as_mut().unwrap();
        resources.insert(Time::new());
        resources.insert(AssetStorage::<Mesh>::new());
        resources.insert(AssetStorage::<Texture>::new());
        resources.insert(AssetStorage::<Shader>::new());
        resources.insert(AssetStorage::<StandardMaterial>::new());
        resources.insert(AssetStorage::<PipelineDescriptor>::new());
        resources.insert(ShaderPipelineAssignments::new());
        resources.insert(CompiledShaderMap::new());
        resources.insert(RenderResourceAssignmentsProvider::default());
        resources.insert(EntityRenderResourceAssignments::default());
        resources.insert(asset_batchers);
        self
    }

    pub fn add_default_systems(&mut self) -> &mut Self {
        self.add_system(build_entity_render_resource_assignments_system());
        self.add_system(ui::ui_update_system::build_ui_update_system());
        for transform_system in
            transform_system_bundle::build(self.world.as_mut().unwrap()).drain(..)
        {
            self.add_system(transform_system);
        }

        self
    }

    pub fn add_render_graph_defaults(&mut self) -> &mut Self {
        self.setup_render_graph(|builder| {
            builder
                .add_draw_target(MeshesDrawTarget::default())
                .add_draw_target(AssignedBatchesDrawTarget::default())
                .add_draw_target(AssignedMeshesDrawTarget::default())
                .add_draw_target(UiDrawTarget::default())
                .add_resource_provider(CameraResourceProvider::default())
                .add_resource_provider(Camera2dResourceProvider::default())
                .add_resource_provider(LightResourceProvider::new(10))
                .add_resource_provider(UiResourceProvider::new())
                .add_resource_provider(MeshResourceProvider::new())
                .add_resource_provider(UniformResourceProviderNew::<StandardMaterial>::new(false))
                .add_resource_provider(UniformResourceProviderNew::<LocalToWorld>::new(false))
                .add_forward_pass()
                .add_forward_pipeline()
                .add_ui_pipeline();
        })
    }

    pub fn setup_render_graph<'a>(
        &'a mut self,
        setup: impl Fn(&'_ mut RenderGraphBuilder<'_>),
    ) -> &'a mut Self {
        {
            let mut render_graph_builder = self
                .render_graph
                .take()
                .unwrap()
                .build(self.resources.as_mut().unwrap());
            setup(&mut render_graph_builder);

            self.render_graph = Some(render_graph_builder.finish());
        }

        self
    }

    #[cfg(feature = "wgpu")]
    pub fn add_wgpu_renderer(&mut self) -> &mut Self {
        self.renderer = Some(Box::new(
            renderer::renderers::wgpu_renderer::WgpuRenderer::new(),
        ));
        self
    }

    #[cfg(not(feature = "wgpu"))]
    fn add_wgpu_renderer(&mut self) -> &mut Self {
        self
    }

    pub fn add_defaults(&mut self) -> &mut Self {
        self.add_default_resources()
            .add_default_systems()
            .add_render_graph_defaults()
            .add_wgpu_renderer()
    }

    pub fn load_plugin(&mut self, path: &str) -> &mut Self {
        let (_lib, plugin) = load_plugin(path);
        plugin.build(self);
        self
    }
}
