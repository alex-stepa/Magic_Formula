// physics.rs — fyzikální vrstva hry.
//
// Tady žijí funkce, které vrací rapier komponenty (RigidBody + Collider).
// V main.rs je jen "přilepíme" na entity přes .insert(...).
// Pozn.: každá funkce vrací `impl Bundle` = skupinu komponent najednou.

use bevy::prelude::*;          // Bundle, Component...
use bevy_rapier3d::prelude::*; // RigidBody, Collider...

// Skupiny srážek: části auta patří do VEHICLE, zem do WORLD.
// Auto i kola se srážejí jen s WORLD → navzájem se IGNORUJÍ (kolo nezaboří do karoserie).
const VEHICLE_GROUP: Group = Group::GROUP_1;
const WORLD_GROUP: Group = Group::GROUP_2;

pub fn ground_physics() -> impl Bundle {
    (
        RigidBody::Fixed,
        Collider::cuboid(25.0, 0.1, 25.0),
        Friction::new(2.0), // víc gripu (mezikrok proti "ledu", než přijde Pacejka)
        // Zem patří do WORLD a sráží se se vším.
        CollisionGroups::new(WORLD_GROUP, Group::ALL),
    )
}

pub fn car_physics() -> impl Bundle {
    (
        RigidBody::Dynamic,
        Collider::cuboid(0.75, 0.25, 2.0), // poloviny užšího těla 1.5×0.5×4
        // Karoserie je MNOHEM těžší než těhlice (poměr ~50:1 jako u reálného auta),
        // jinak by reakce motoru řízení otáčela celým autem místo natáčení kol.
        AdditionalMassProperties::Mass(50.0),
        // Auto patří do VEHICLE a sráží se jen s WORLD (ne s vlastními koly).
        CollisionGroups::new(VEHICLE_GROUP, WORLD_GROUP),
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
        Friction::new(2.0), // víc gripu (mezikrok proti "ledu", než přijde Pacejka)
        // Kolo patří do VEHICLE a sráží se jen s WORLD (ne s karoserií ani druhými koly).
        CollisionGroups::new(VEHICLE_GROUP, WORLD_GROUP),
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

/// Joint kola. Kolo se valí kolem osy X (volná osa).
/// `steered` = řízené kolo. U něj použijeme generic joint, jehož rám (local_basis1)
/// za běhu natáčíme kolem Y → KINEMATICKÉ řízení: osa valení se natočí geometrií,
/// ne silovým motorem. Proto se točí kolo, ne trup, a nic se neklepe.
/// `mount` = bod uchycení na chassis.
pub fn wheel_joint(suspension: Suspension, chassis: Entity, mount: Vec3, steered: bool) -> ImpulseJoint {
    match suspension {
        Suspension::Rigid => {
            if steered {
                // LOCKED_REVOLUTE_AXES = volné je jen valení (ANG_X), zbytek zamčený.
                // Řízení mění local_basis1 za běhu (viz vehicle_steer).
                let joint = GenericJointBuilder::new(JointAxesMask::LOCKED_REVOLUTE_AXES)
                    .local_anchor1(mount)
                    .local_anchor2(Vec3::ZERO)
                    .local_basis1(Quat::IDENTITY) // start rovně
                    .build();
                ImpulseJoint::new(chassis, TypedJoint::GenericJoint(joint))
            } else {
                let joint = RevoluteJointBuilder::new(Vec3::X)
                    .local_anchor1(mount)
                    .local_anchor2(Vec3::ZERO);
                ImpulseJoint::new(chassis, joint)
            }
        }
    }
}