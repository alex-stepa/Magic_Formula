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

/// Fyzika pro jedno kolo.
///
/// Bere parametry, ne napevno daná čísla — až přijde build system,
/// `radius` a `width` se budou číst z `.veh` souboru.
///
/// `Collider::cylinder(half_height, radius)`:
///   - half_height = POLOVINA šířky kola (zase poloviny, jako u kvádru!)
///   - radius      = poloměr kola
/// Osu válce (zatím míří na Y) srovnáme na X až při spawnu otočením celého kola.
pub fn wheel_physics(radius: f32, width: f32) -> impl Bundle {
    (
        RigidBody::Dynamic, // kolo se hýbe, točí, pérování
        // Tělo kola necháváme NEotočené (kvůli ose jointu). Válec proto otáčíme
        // jen UVNITŘ collideru přes compound: jeden podtvar pootočený o 90° kolem Z,
        // takže osa válce míří na X (osa otáčení), ale tělo zůstává srovnané.
        Collider::compound(vec![(
            Vec3::ZERO,                                       // podtvar je ve středu těla
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2), // pootočení podtvaru o 90°
            Collider::cylinder(width / 2.0, radius),          // půlka šířky = half_height
        )]),
    )
}

/// Typ zavěšení kola. Je to DATA — build system i Fáze 2 (suspension geometry)
/// budou jen měnit, který typ kolo má. Stavbu jointu řeší `wheel_joint()`.
#[derive(Component, Clone, Copy)]
pub enum Suspension {
    Rigid, // napevno: kolo se jen točí, žádné pérování (náš první krok)
    // Budoucí typy (každý si přidá vlastní parametry a větev ve wheel_joint):
    // McPherson { stiffness: f32, damping: f32, travel: f32 },
    // DoubleWishbone { ... },
    // LeafSpring { ... },
}

/// Postaví joint, kterým se kolo připne k chassis.
/// JEDINÉ místo, co převádí typ zavěšení na rapier joint — přidat nový typ
/// znamená přidat větev do `match`, zbytek hry (spawn smyčka) se nemění.
///
/// `chassis` = entita auta, ke které kolo připínáme.
/// `mount`   = bod na chassis (v jeho lokálních souřadnicích), kde kolo sedí.
pub fn wheel_joint(suspension: Suspension, chassis: Entity, mount: Vec3) -> ImpulseJoint {
    match suspension {
        Suspension::Rigid => {
            // Revolute = kloub s jednou volnou osou otáčení. Vec3::X = osa kola.
            // local_anchor1 = bod na chassis, local_anchor2 = střed kola.
            let joint = RevoluteJointBuilder::new(Vec3::X)
                .local_anchor1(mount)
                .local_anchor2(Vec3::ZERO);
            ImpulseJoint::new(chassis, joint)
        }
    }
}