//! The `GenerationMap` type for storing immutable, generated objects.

use std::any::TypeId;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use slotmap::{Key, SlotMap};

use crate::component::{serialize_params, Component};
use crate::deps::arcstr::ArcStr;
use crate::error::{ErrorSource, Result as SubResult};

/// Structure for keeping track of immutable objects, some of which should be generated only once.
#[derive(Debug)]
pub(crate) struct GenerationMap<K, S, V>
where
    S: Key,
{
    /// Mapping from key representing generator parameters to a generated object identifier.
    target_map: HashMap<K, S>,
    /// Map from name to a generated object identifier.
    name_map: HashMap<ArcStr, S>,
    /// Mapping from internal key to a generated object.
    objects: SlotMap<S, ObjectStatus<V>>,
}

/// Type for returning whether an item needs to be generated.
/// Both `R` and `S` should be inexpensive to clone.
#[derive(Clone)]
pub(crate) enum GeneratedCheck<R, S>
where
    R: Clone,
    S: Clone,
{
    /// The item of type `R` being requested already exists.
    Exists(R),
    /// The item being requested does not exist and has been assigned an ID of type `S`.
    MustGenerate(S),
}

/// Type for storing objects.
#[derive(Debug)]
pub(crate) enum ObjectStatus<V> {
    /// The item of type `V` exists.
    Exists(Arc<V>),
    /// The item is currently loading (i.e. it has been assigned an ID but is still pending a
    /// value).
    Loading,
}

impl<K, S, V> GenerationMap<K, S, V>
where
    K: Eq + Hash,
    S: Key,
{
    /// Creates a new [`GenerationMap`].
    pub(crate) fn new() -> Self {
        Self {
            target_map: HashMap::new(),
            name_map: HashMap::new(),
            objects: SlotMap::with_key(),
        }
    }

    /// Gets an identifier for a given key, creating one if it does not yet exist.
    ///
    /// # Examples
    ///
    /// See unit tests for examples.
    pub(crate) fn get_id(&mut self, key: K) -> GeneratedCheck<S, S> {
        match self.target_map.entry(key) {
            Entry::Occupied(o) => GeneratedCheck::Exists(*o.get()),
            Entry::Vacant(v) => {
                let mkey = self.objects.insert(ObjectStatus::Loading);
                v.insert(mkey);
                GeneratedCheck::MustGenerate(mkey)
            }
        }
    }

    /// Gets a generated object by its unique identifier.
    ///
    /// # Examples
    ///
    /// See unit tests for examples.
    pub(crate) fn get_by_id(&self, id: S) -> SubResult<&Arc<V>> {
        match self.objects[id] {
            ObjectStatus::Loading => Err(ErrorSource::Internal(
                "attempted to view object before it has been loaded".to_string(),
            )
            .into()),
            ObjectStatus::Exists(ref v) => Ok(v),
        }
    }

    /// Gets a object generated with the given parameters, panicking if the object is currently
    /// being generated by another thread.
    ///
    /// Returns a new identifier if object generation has not yet started and marks the object
    /// with [`ObjectStatus::Loading`].
    ///
    /// # Examples
    ///
    /// See unit tests for examples.
    pub(crate) fn get(&mut self, key: K) -> GeneratedCheck<Arc<V>, S> {
        match self.get_id(key) {
            GeneratedCheck::Exists(id) => GeneratedCheck::Exists(
                self.get_by_id(id)
                    .expect("object should be already have been generated")
                    .clone(),
            ),
            GeneratedCheck::MustGenerate(id) => GeneratedCheck::MustGenerate(id),
        }
    }

    /// Generates a new identifier and marks the corresponding object with
    /// [`ObjectStatus::Loading`].
    ///
    /// Used for bypassing generation logic for objects that are not parametrized (e.g. imported
    /// cells)
    ///
    /// # Examples
    ///
    /// See unit tests for examples.
    pub(crate) fn gen_id(&mut self) -> S {
        self.objects.insert(ObjectStatus::Loading)
    }

    /// Sets the value for an object with ID `id` after it has been loaded.
    ///
    /// # Examples
    ///
    /// See unit tests for examples.
    pub(crate) fn set(&mut self, id: S, name: impl Into<ArcStr>, value: V) -> Arc<V> {
        let arc = Arc::new(value);
        self.objects[id] = ObjectStatus::Exists(arc.clone());
        self.name_map.insert(name.into(), id);
        arc
    }

    /// Allocates an unused name derived from the given base name.
    ///
    /// Does not reserve the name in any way. It is up to the caller to
    /// use the name immediately upon allocation, or else other callers
    /// may be issued the same name.
    pub(crate) fn alloc_name(&self, base_name: impl Into<ArcStr>) -> ArcStr {
        let base_name = base_name.into();
        if self.is_name_available(&base_name) {
            return base_name;
        }

        let mut i = 2;
        loop {
            let name = arcstr::format!("{}_{}", base_name, i);
            if self.is_name_available(&name) {
                break name;
            }
            i += 1;
        }
    }

    /// Checks whether or not the given name is in use.
    #[inline]
    pub(crate) fn is_name_used(&self, name: &str) -> bool {
        self.name_map.contains_key(name)
    }

    /// Checks whether or not the given name is available.
    #[inline]
    pub(crate) fn is_name_available(&self, name: &str) -> bool {
        !self.is_name_used(name)
    }

    /// Iterates over the values in the map.
    ///
    /// # Examples
    ///
    /// See unit tests for examples.
    pub(crate) fn values(&self) -> impl Iterator<Item = &Arc<V>> {
        self.objects.values().filter_map(|v| match v {
            ObjectStatus::Exists(v) => Some(v),
            ObjectStatus::Loading => None,
        })
    }
}

/// Key for uniquely identifying generated [`Component`]s.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) struct ParamKey {
    /// An identifier for a [`Component`] type.
    t: TypeId,
    /// Serialized parameters for the given [`Component`] type.
    params: Vec<u8>,
}

impl ParamKey {
    /// Creates a new [`ParamKey`].
    pub fn new(t: TypeId, params: impl Into<Vec<u8>>) -> Self {
        Self {
            t,
            params: params.into(),
        }
    }

    /// Creates a new [`ParamKey`] from a SubComponent's parameters.
    pub fn from_params<T>(params: &T::Params) -> Self
    where
        T: Component,
    {
        Self::new(TypeId::of::<T>(), serialize_params(params))
    }
}

#[cfg(test)]
mod tests {
    use slotmap::new_key_type;

    use super::*;

    new_key_type!(
        struct TestKey;
    );

    #[test]
    fn test_generation_map_get_id() {
        let mut gen_map = GenerationMap::new();

        let id: TestKey = match gen_map.get_id("key1".to_string()) {
            GeneratedCheck::Exists(_) => panic!("Corresponding object should not exist already"),
            GeneratedCheck::MustGenerate(id) => id,
        };

        gen_map.set(id, "name", "value".to_string());

        let same_id = match gen_map.get_id("key1".to_string()) {
            GeneratedCheck::Exists(id) => id,
            GeneratedCheck::MustGenerate(_) => panic!("Corresponding object should exist already"),
        };

        assert_eq!(id, same_id);

        let new_id = match gen_map.get_id("key2".to_string()) {
            GeneratedCheck::Exists(_) => panic!("Corresponding object should not exist already"),
            GeneratedCheck::MustGenerate(id) => id,
        };

        assert_ne!(id, new_id);
    }

    #[test]
    fn test_generation_map_get_by_id() -> SubResult<()> {
        let mut gen_map: GenerationMap<String, _, _> = GenerationMap::new();

        let id: TestKey = gen_map.gen_id();

        gen_map.set(id, "name", "value".to_string());

        let v = gen_map.get_by_id(id)?;

        assert_eq!(v, &Arc::new("value".to_string()));

        Ok(())
    }

    #[test]
    fn test_generation_map_get() {
        let mut gen_map = GenerationMap::new();

        let id: TestKey = match gen_map.get("key1".to_string()) {
            GeneratedCheck::Exists(_) => panic!("Corresponding object should not exist already"),
            GeneratedCheck::MustGenerate(id) => id,
        };

        gen_map.set(id, "name", "value".to_string());

        let v = match gen_map.get("key1".to_string()) {
            GeneratedCheck::Exists(v) => v,
            GeneratedCheck::MustGenerate(_) => panic!("Corresponding object should exist already"),
        };

        assert_eq!(v, Arc::from("value".to_string()));

        let new_id = match gen_map.get("key2".to_string()) {
            GeneratedCheck::Exists(_) => panic!("Corresponding object should not exist already"),
            GeneratedCheck::MustGenerate(id) => id,
        };

        assert_ne!(id, new_id);
    }

    #[test]
    fn test_generation_map_values() {
        let mut gen_map: GenerationMap<String, _, _> = GenerationMap::new();

        for i in 0..3 {
            let id: TestKey = gen_map.gen_id();

            gen_map.set(id, "name", format!("value{i}"));
        }

        for _ in 0..3 {
            let _ = gen_map.gen_id();
        }

        let values = gen_map.values().collect::<Vec<_>>();

        assert_eq!(values.len(), 3);
        assert!(values.contains(&&Arc::new("value0".to_string())));
        assert!(values.contains(&&Arc::new("value1".to_string())));
        assert!(values.contains(&&Arc::new("value2".to_string())));
    }
}
