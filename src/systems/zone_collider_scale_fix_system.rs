use bevy::prelude::{Added, DetectChangesMut, Query};
use bevy_rapier3d::prelude::{Collider, RapierColliderHandle};

/// Workaround for bevy_rapier3d 0.22.0 system ordering race condition:
///
/// When `AsyncCollider` creates a `Collider` (via `init_async_colliders`), the
/// `apply_scale` system has already run for that frame. By the time `init_colliders`
/// registers the collider with rapier (inserting `RapierColliderHandle`), the
/// `Changed<Collider>` flag has expired. This means `apply_scale` never fires for
/// AsyncCollider-created colliders, leaving their shapes at raw mesh scale instead
/// of being scaled by the entity's GlobalTransform.
///
/// This system detects newly-registered colliders and marks their `Collider`
/// component as changed, causing `apply_scale` to pick them up on the next frame
/// and correctly apply the GlobalTransform scale.
pub fn zone_collider_scale_fix_system(
    mut colliders: Query<&mut Collider, Added<RapierColliderHandle>>,
) {
    for mut collider in colliders.iter_mut() {
        collider.set_changed();
    }
}
