//! `EntityWorld` (spec §3) au-dessus de `hecs`.
//!
//! - ids Java monotones ([`EntityId`]), jamais réutilisés ;
//! - `query_ids` renvoie une **copie** triée par id croissant (décision de Gabriel : ordre
//!   déterministe au lieu de l'émulation du `HashMap<Integer>` Java, B1) ;
//! - détruire une entité pendant le parcours d'une telle liste est sûr ; les accès ultérieurs à
//!   une entité détruite renvoient `None`.

use std::collections::BTreeMap;

use hecs::{Component, Query};
use nb_core::glam::Vec3;

use crate::components::EntityId;

#[derive(Default)]
pub struct EntityWorld {
    world: hecs::World,
    next_id: i32,
    by_id: BTreeMap<EntityId, hecs::Entity>,
    /// Position (yeux) fournie au `MobSpawnSystem` avant chaque tick (`setPlayerPosition`).
    pub spawn_origin: Vec3,
    /// Positions (yeux) des joueurs distants d'une partie LAN : le `MobSpawnSystem` fait aussi
    /// apparaître et disparaître des mobs autour d'eux.
    pub extra_origins: Vec<Vec3>,
}

impl std::fmt::Debug for EntityWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityWorld")
            .field("next_id", &self.next_id)
            .field("len", &self.by_id.len())
            .finish_non_exhaustive()
    }
}

impl EntityWorld {
    pub fn new() -> Self {
        Self::default()
    }

    /// `createEntity()` : renvoie `nextId++`.
    pub fn create(&mut self) -> EntityId {
        let id = EntityId(self.next_id);
        self.next_id += 1;
        let e = self.world.spawn((id,));
        self.by_id.insert(id, e);
        id
    }

    /// `destroyEntity(id)` : retire l'entité et tous ses composants (inconnue → rien).
    pub fn destroy(&mut self, id: EntityId) {
        if let Some(e) = self.by_id.remove(&id) {
            let _ = self.world.despawn(e);
        }
    }

    pub fn contains(&self, id: EntityId) -> bool {
        self.by_id.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Prochain id qui sera attribué.
    pub fn next_id(&self) -> i32 {
        self.next_id
    }

    /// `addComponent` : remplace un composant du même type ; entité inconnue → ignoré.
    pub fn insert<C: Component>(&mut self, id: EntityId, c: C) {
        if let Some(&e) = self.by_id.get(&id) {
            let _ = self.world.insert_one(e, c);
        }
    }

    /// Retire un composant (s'il existe).
    pub fn remove<C: Component>(&mut self, id: EntityId) -> Option<C> {
        let e = *self.by_id.get(&id)?;
        self.world.remove_one::<C>(e).ok()
    }

    /// `getComponent` (emprunt partagé dynamique).
    pub fn get<C: Component>(&self, id: EntityId) -> Option<hecs::Ref<'_, C>> {
        let e = *self.by_id.get(&id)?;
        self.world.get::<&C>(e).ok()
    }

    /// Accès exclusif à un composant.
    pub fn get_mut<C: Component>(&mut self, id: EntityId) -> Option<&mut C> {
        let e = *self.by_id.get(&id)?;
        self.world.query_one_mut::<&mut C>(e).ok()
    }

    /// Copie d'un composant `Copy`.
    pub fn copy<C: Component + Copy>(&self, id: EntityId) -> Option<C> {
        self.get::<C>(id).map(|r| *r)
    }

    /// `hasComponent`.
    pub fn has<C: Component>(&self, id: EntityId) -> bool {
        self.by_id
            .get(&id)
            .and_then(|&e| self.world.entity(e).ok())
            .is_some_and(|r| r.has::<C>())
    }

    /// Requête sur une seule entité (plusieurs composants empruntés en même temps).
    pub fn query_one_mut<Q: Query>(&mut self, id: EntityId) -> Option<Q::Item<'_>> {
        let e = *self.by_id.get(&id)?;
        self.world.query_one_mut::<Q>(e).ok()
    }

    /// Requête sur deux entités distinctes à la fois (panique si `a == b`).
    pub fn query_pair_mut<Q: Query>(
        &mut self,
        a: EntityId,
        b: EntityId,
    ) -> Option<(Q::Item<'_>, Q::Item<'_>)> {
        let ea = *self.by_id.get(&a)?;
        let eb = *self.by_id.get(&b)?;
        let [ra, rb] = self.world.query_many_mut::<Q, 2>([ea, eb]);
        Some((ra.ok()?, rb.ok()?))
    }

    /// `query(T1, T2, …)` : ids possédant tous les composants de `Q`, triés par id croissant.
    pub fn query_ids<Q: Query>(&self) -> Vec<EntityId> {
        self.by_id
            .iter()
            .filter(|(_, &e)| self.world.satisfies::<Q>(e).unwrap_or(false))
            .map(|(&id, _)| id)
            .collect()
    }

    /// Tous les ids vivants, triés.
    pub fn ids(&self) -> Vec<EntityId> {
        self.by_id.keys().copied().collect()
    }

    /// Monde `hecs` sous-jacent (lecture seule : rendu, sauvegarde).
    pub fn raw(&self) -> &hecs::World {
        &self.world
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Health, Transform, Velocity};

    #[test]
    fn ids_are_monotonic_and_never_reused() {
        let mut w = EntityWorld::new();
        let a = w.create();
        let b = w.create();
        assert_eq!((a, b), (EntityId(0), EntityId(1)));
        w.destroy(a);
        let c = w.create();
        assert_eq!(c, EntityId(2));
        assert!(!w.contains(a));
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn insert_replaces_and_ignores_dead_entities() {
        let mut w = EntityWorld::new();
        let a = w.create();
        w.insert(a, Health::new(5.0));
        w.insert(a, Health::new(7.0));
        assert_eq!(w.copy::<Health>(a).unwrap().max_health, 7.0);
        w.destroy(a);
        w.insert(a, Health::new(1.0));
        assert!(w.get::<Health>(a).is_none());
        assert!(!w.has::<Health>(a));
        assert!(w.get_mut::<Health>(a).is_none());
    }

    #[test]
    fn query_is_sorted_snapshot() {
        let mut w = EntityWorld::new();
        let ids: Vec<_> = (0..40).map(|_| w.create()).collect();
        for (i, &id) in ids.iter().enumerate().rev() {
            w.insert(id, Transform::new(i as f64, 0.0, 0.0));
            if i % 2 == 0 {
                w.insert(id, Velocity::default());
            }
        }
        let q = w.query_ids::<(&Transform, &Velocity)>();
        assert_eq!(q.len(), 20);
        assert!(q.windows(2).all(|p| p[0] < p[1]));
        // Détruire pendant le parcours est sûr ; une entité détruite n'est plus accessible.
        let mut touched = 0;
        for id in &q {
            if let Some(t) = w.get_mut::<Transform>(*id) {
                t.pos.y = 1.0;
                touched += 1;
            }
            w.destroy(EntityId(id.0 + 2));
        }
        assert_eq!(touched, 1);
        assert_eq!(w.query_ids::<&Velocity>(), vec![EntityId(0)]);
        assert_eq!(w.copy::<Transform>(EntityId(0)).unwrap().pos.y, 1.0);
    }

    #[test]
    fn pair_access() {
        let mut w = EntityWorld::new();
        let a = w.create();
        let b = w.create();
        w.insert(a, Transform::new(1.0, 0.0, 0.0));
        w.insert(b, Transform::new(2.0, 0.0, 0.0));
        let (ta, tb) = w.query_pair_mut::<&mut Transform>(a, b).unwrap();
        std::mem::swap(&mut ta.pos, &mut tb.pos);
        assert_eq!(w.copy::<Transform>(a).unwrap().pos.x, 2.0);
    }
}
