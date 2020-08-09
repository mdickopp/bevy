use super::{FromResources, Resources};
use crate::{
    system::{SystemId, TypeAccess},
    Resource, ResourceIndex,
};
use core::{
    any::TypeId,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};
use hecs::smaller_tuples_too;
use std::marker::PhantomData;

/// Shared borrow of a Resource
pub struct Res<'a, T: Resource> {
    value: &'a T,
}

impl<'a, T: Resource> Res<'a, T> {
    pub unsafe fn new(value: NonNull<T>) -> Self {
        Self {
            value: &*value.as_ptr(),
        }
    }
}

/// A clone that is unsafe to perform. You probably shouldn't use this.
pub trait UnsafeClone {
    unsafe fn unsafe_clone(&self) -> Self;
}

impl<'a, T: Resource> UnsafeClone for Res<'a, T> {
    unsafe fn unsafe_clone(&self) -> Self {
        Self { value: self.value }
    }
}

unsafe impl<T: Resource> Send for Res<'_, T> {}
unsafe impl<T: Resource> Sync for Res<'_, T> {}

impl<'a, T: Resource> Deref for Res<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value
    }
}

/// Unique borrow of a Resource
pub struct ResMut<'a, T: Resource> {
    _marker: PhantomData<&'a T>,
    value: *mut T,
}

impl<'a, T: Resource> ResMut<'a, T> {
    pub unsafe fn new(value: NonNull<T>) -> Self {
        Self {
            value: value.as_ptr(),
            _marker: Default::default(),
        }
    }
}

unsafe impl<T: Resource> Send for ResMut<'_, T> {}
unsafe impl<T: Resource> Sync for ResMut<'_, T> {}

impl<'a, T: Resource> Deref for ResMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.value }
    }
}

impl<'a, T: Resource> DerefMut for ResMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value }
    }
}

impl<'a, T: Resource> UnsafeClone for ResMut<'a, T> {
    unsafe fn unsafe_clone(&self) -> Self {
        Self {
            value: self.value,
            _marker: Default::default(),
        }
    }
}

/// Local<T> resources are unique per-system. Two instances of the same system will each have their own resource.
/// Local resources are automatically initialized using the FromResources trait.
pub struct Local<'a, T: Resource + FromResources> {
    value: *mut T,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Resource + FromResources> UnsafeClone for Local<'a, T> {
    unsafe fn unsafe_clone(&self) -> Self {
        Self {
            value: self.value,
            _marker: Default::default(),
        }
    }
}

impl<'a, T: Resource + FromResources> Deref for Local<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.value }
    }
}

impl<'a, T: Resource + FromResources> DerefMut for Local<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value }
    }
}

/// A collection of resource types fetch from a `Resources` collection
pub trait ResourceQuery {
    type Fetch: for<'a> FetchResource<'a>;

    fn initialize(_resources: &mut Resources, _system_id: Option<SystemId>) {}
}

/// Streaming iterators over contiguous homogeneous ranges of resources
pub trait FetchResource<'a>: Sized {
    /// Type of value to be fetched
    type Item: UnsafeClone;

    fn access() -> TypeAccess;
    fn borrow(resources: &Resources);
    fn release(resources: &Resources);

    unsafe fn get(resources: &'a Resources, system_id: Option<SystemId>) -> Self::Item;
}

impl<'a, T: Resource> ResourceQuery for Res<'a, T> {
    type Fetch = FetchResourceRead<T>;
}

/// Fetches a shared resource reference
pub struct FetchResourceRead<T>(NonNull<T>);

impl<'a, T: Resource> FetchResource<'a> for FetchResourceRead<T> {
    type Item = Res<'a, T>;

    unsafe fn get(resources: &'a Resources, _system_id: Option<SystemId>) -> Self::Item {
        Res::new(resources.get_unsafe_ref::<T>(ResourceIndex::Global))
    }

    fn borrow(resources: &Resources) {
        resources.borrow::<T>();
    }

    fn release(resources: &Resources) {
        resources.release::<T>();
    }

    fn access() -> TypeAccess {
        let mut access = TypeAccess::default();
        access.immutable.insert(TypeId::of::<T>());
        access
    }
}

impl<'a, T: Resource> ResourceQuery for ResMut<'a, T> {
    type Fetch = FetchResourceWrite<T>;
}

/// Fetches a unique resource reference
pub struct FetchResourceWrite<T>(NonNull<T>);

impl<'a, T: Resource> FetchResource<'a> for FetchResourceWrite<T> {
    type Item = ResMut<'a, T>;

    unsafe fn get(resources: &'a Resources, _system_id: Option<SystemId>) -> Self::Item {
        ResMut::new(resources.get_unsafe_ref::<T>(ResourceIndex::Global))
    }

    fn borrow(resources: &Resources) {
        resources.borrow_mut::<T>();
    }

    fn release(resources: &Resources) {
        resources.release_mut::<T>();
    }

    fn access() -> TypeAccess {
        let mut access = TypeAccess::default();
        access.mutable.insert(TypeId::of::<T>());
        access
    }
}

impl<'a, T: Resource + FromResources> ResourceQuery for Local<'a, T> {
    type Fetch = FetchResourceLocalMut<T>;

    fn initialize(resources: &mut Resources, id: Option<SystemId>) {
        let value = T::from_resources(resources);
        let id = id.expect("Local<T> resources can only be used by systems");
        resources.insert_local(id, value);
    }
}

/// Fetches a `Local<T>` resource reference
pub struct FetchResourceLocalMut<T>(NonNull<T>);

impl<'a, T: Resource + FromResources> FetchResource<'a> for FetchResourceLocalMut<T> {
    type Item = Local<'a, T>;

    unsafe fn get(resources: &'a Resources, system_id: Option<SystemId>) -> Self::Item {
        let id = system_id.expect("Local<T> resources can only be used by systems");
        Local {
            value: resources
                .get_unsafe_ref::<T>(ResourceIndex::System(id))
                .as_ptr(),
            _marker: Default::default(),
        }
    }

    fn borrow(resources: &Resources) {
        resources.borrow_mut::<T>();
    }

    fn release(resources: &Resources) {
        resources.release_mut::<T>();
    }

    fn access() -> TypeAccess {
        let mut access = TypeAccess::default();
        access.mutable.insert(TypeId::of::<T>());
        access
    }
}

macro_rules! tuple_impl {
    ($($name: ident),*) => {
        impl<'a, $($name: FetchResource<'a>),*> FetchResource<'a> for ($($name,)*) {
            type Item = ($($name::Item,)*);

            #[allow(unused_variables)]
            fn borrow(resources: &Resources) {
                $($name::borrow(resources);)*
            }

            #[allow(unused_variables)]
            fn release(resources: &Resources) {
                $($name::release(resources);)*
            }

            #[allow(unused_variables)]
            unsafe fn get(resources: &'a Resources, system_id: Option<SystemId>) -> Self::Item {
                ($($name::get(resources, system_id),)*)
            }

            #[allow(unused_mut)]
            fn access() -> TypeAccess {
                let mut access = TypeAccess::default();
                $(access.union(&$name::access());)*
                access
            }
        }

        impl<$($name: ResourceQuery),*> ResourceQuery for ($($name,)*) {
            type Fetch = ($($name::Fetch,)*);

            #[allow(unused_variables)]
            fn initialize(resources: &mut Resources, system_id: Option<SystemId>) {
                $($name::initialize(resources, system_id);)*
            }
        }

        #[allow(unused_variables)]
        #[allow(non_snake_case)]
        impl<$($name: UnsafeClone),*> UnsafeClone for ($($name,)*) {
            unsafe fn unsafe_clone(&self) -> Self {
                let ($($name,)*) = self;
                ($($name.unsafe_clone(),)*)
            }
        }
    };
}

smaller_tuples_too!(tuple_impl, O, N, M, L, K, J, I, H, G, F, E, D, C, B, A);
