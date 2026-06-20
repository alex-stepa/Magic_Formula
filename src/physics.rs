// physics.rs — rapier komponenty pro STATICKÉ prvky světa.
//
// Pozn.: vozidla už NEjedou přes rapier — auto i soft-body běží na vlastním
// node-beam solveru (viz `SoftBody` v main.rs). Rapier tu zůstal jen pro zem
// (a další pevné překážky v budoucnu), proti které solver kontaktuje.
// Každá funkce vrací `impl Bundle` = skupinu komponent, kterou v main.rs přilepíme.

use bevy::prelude::*;          // Bundle, Component...
use bevy_rapier3d::prelude::*; // RigidBody, Collider...

// Zem patří do skupiny WORLD a sráží se se vším.
const WORLD_GROUP: Group = Group::GROUP_2;

pub fn ground_physics() -> impl Bundle {
    (
        RigidBody::Fixed,
        Collider::cuboid(25.0, 0.1, 25.0),
        Friction::new(2.0), // víc gripu (mezikrok proti "ledu", než přijde Pacejka)
        CollisionGroups::new(WORLD_GROUP, Group::ALL),
    )
}
