#![deny(future_incompatible, nonstandard_style)]
#![warn(missing_docs, rust_2018_idioms, clippy::pedantic)]
#![allow(
    clippy::needless_pass_by_value,
    clippy::type_complexity,
    clippy::too_many_arguments
)]
#![cfg(any(dim2, dim3))]

//! Physics behavior for Heron, using [rapier](https://rapier.rs/)
//!
//! # Supported custom collision shapes
//!
//! The following types are accepted as [`heron_core::CustomCollisionShape`]
//! values.
//!
//! - [`rapier::geometry::ColliderBuilder`]

#[cfg(feature = "rapier2d")]
pub extern crate rapier2d;
#[cfg(feature = "rapier3d")]
pub extern crate rapier3d;

use bevy::ecs::component::Component;
use bevy::prelude::*;
#[cfg(dim2)]
pub(crate) use rapier2d as rapier;
#[cfg(dim3)]
pub(crate) use rapier3d as rapier;

use heron_core::{CollisionEvent, PhysicsSystem};
pub use pipeline::{PhysicsWorld, RayCastInfo, ShapeCastCollisionInfo, ShapeCastCollisionType};

use crate::rapier::dynamics::{
    self, CCDSolver, IntegrationParameters, IslandManager, JointSet, RigidBodySet,
};
use crate::rapier::geometry::{self, BroadPhase, ColliderSet, NarrowPhase};
pub use crate::rapier::na as nalgebra;
use crate::rapier::pipeline::{PhysicsPipeline, QueryPipeline};

mod acceleration;
mod body;
pub mod convert;
mod damping;
mod pipeline;
mod shape;
mod velocity;

/// Plugin that enables collision detection and physics behavior, powered by rapier.
#[must_use]
#[derive(Debug, Copy, Clone, Default)]
pub struct RapierPlugin;

/// Component that holds a reference to the rapier rigid body
///
/// It is automatically inserted and removed by heron.
/// It is only useful for advanced, direct access to the rapier world
#[derive(Debug, Copy, Clone, Eq, PartialEq, Component)]
pub struct RigidBodyHandle(dynamics::RigidBodyHandle);

/// Component that holds a reference to the rapier collider
///
/// It is automatically inserted and removed by heron.
/// It is only useful for advanced, direct access to the rapier world
#[derive(Debug, Copy, Clone, Eq, PartialEq, Component)]
pub struct ColliderHandle(geometry::ColliderHandle);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, SystemLabel)]
enum InternalSystem {
    TransformPropagation,
}

impl Plugin for RapierPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(heron_core::CorePlugin)
            .init_resource::<PhysicsPipeline>()
            .init_resource::<body::HandleMap>()
            .init_resource::<shape::HandleMap>()
            .init_resource::<IntegrationParameters>()
            .add_event::<CollisionEvent>()
            .insert_resource(BroadPhase::new())
            .insert_resource(NarrowPhase::new())
            .insert_resource(RigidBodySet::new())
            .insert_resource(QueryPipeline::new())
            .insert_resource(IslandManager::new())
            .insert_resource(ColliderSet::new())
            .insert_resource(JointSet::new())
            .insert_resource(CCDSolver::new())
            .stage("heron-physics", |schedule: &mut Schedule| {
                schedule
                    .add_stage("heron-remove", removal_stage())
                    .add_stage("heron-update-rapier-world", update_rapier_world_stage())
                    .add_stage("heron-create-new-bodies", body_update_stage())
                    .add_stage("heron-create-new-colliders", create_collider_stage())
            })
            .add_system_set_to_stage(CoreStage::PostUpdate, step_systems());
    }
}

fn removal_stage() -> SystemStage {
    SystemStage::single_threaded()
        .with_system(body::remove_invalids_after_components_removed)
        .with_system(shape::remove_invalids_after_components_removed)
        .with_system(body::remove_invalids_after_component_changed)
        .with_system(shape::remove_invalids_after_component_changed)
}

fn update_rapier_world_stage() -> SystemStage {
    SystemStage::parallel()
        .with_system(
            bevy::transform::transform_propagate_system::transform_propagate_system
                .label(InternalSystem::TransformPropagation),
        )
        .with_system(body::update_rapier_position.after(InternalSystem::TransformPropagation))
        .with_system(velocity::update_rapier_velocity)
        .with_system(acceleration::update_rapier_force_and_torque)
        .with_system(damping::update_rapier_damping)
        .with_system(damping::reset_rapier_damping)
        .with_system(shape::update_position)
        .with_system(shape::update_collision_groups)
        .with_system(shape::update_sensor_flag)
        .with_system(shape::remove_sensor_flag)
        .with_system(shape::reset_collision_groups)
}

fn body_update_stage() -> SystemStage {
    SystemStage::single_threaded()
        .with_run_criteria(heron_core::should_run)
        .with_system(body::create)
}

fn create_collider_stage() -> SystemStage {
    SystemStage::single_threaded()
        .with_run_criteria(heron_core::should_run)
        .with_system(shape::create)
}

fn step_systems() -> SystemSet {
    SystemSet::new()
        .with_run_criteria(heron_core::should_run)
        .with_system(pipeline::update_integration_parameters.before(PhysicsSystem::Events))
        .with_system(pipeline::step.label(PhysicsSystem::Events))
        .with_system(
            body::update_bevy_transform
                .label(PhysicsSystem::TransformUpdate)
                .after(PhysicsSystem::Events),
        )
        .with_system(
            velocity::update_velocity_component
                .label(PhysicsSystem::VelocityUpdate)
                .after(PhysicsSystem::Events),
        )
        .with_system(
            velocity::update_rapier_velocity
                .after(PhysicsSystem::Events)
                .after(PhysicsSystem::VelocityUpdate),
        )
}
