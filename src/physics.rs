// physics.rs — fyzikální vrstva hry.
//
// Tady žijí funkce, které vrací rapier komponenty (RigidBody + Collider).
// V main.rs je jen "přilepíme" na entity přes .insert(...).
// Pozn.: každá funkce vrací `impl Bundle` = skupinu komponent najednou.

use bevy::prelude::*;          // Bundle, Component...
use bevy_rapier3d::prelude::*; // RigidBody, Collider...

// TODO: ground_physics() -> impl Bundle  (RigidBody::Fixed + Collider zem)
// TODO: car_physics()    -> impl Bundle  (RigidBody::Dynamic + Collider auto)

pub fn ground_physics() -> impl Bundle {
    (
        RigidBody::Fixed,
        Collider::cuboid(25.0, 0.1, 25.0),
    )
}

pub fn car_physics() -> impl Bundle {
    (
        RigidBody::Dynamic,
        Collider::cuboid(1.0, 0.25, 2.0),
    )
}