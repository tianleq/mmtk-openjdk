use crate::reference_glue::DISCOVERED_LISTS;
use crate::OpenJDK;
use crate::OpenJDKSlot;

use super::abi::*;
use super::UPCALLS;
use mmtk::util::opaque_pointer::*;
use mmtk::util::{Address, ObjectReference};
use mmtk::vm::slot::Slot;
use mmtk::vm::SlotVisitor;
use std::cell::UnsafeCell;
use std::{mem, slice};

type S<const COMPRESSED: bool> = OpenJDKSlot<COMPRESSED>;

trait OopIterate: Sized {
    fn oop_iterate<const COMPRESSED: bool>(
        &self,
        oop: Oop,
        closure: &mut impl SlotVisitor<OpenJDKSlot<COMPRESSED>>,
    );
}

impl OopIterate for OopMapBlock {
    fn oop_iterate<const COMPRESSED: bool>(
        &self,
        oop: Oop,
        closure: &mut impl SlotVisitor<S<COMPRESSED>>,
    ) {
        let log_bytes_in_oop = if COMPRESSED { 2 } else { 3 };
        let start = oop.get_field_address(self.offset);
        let _source = ObjectReference::from(oop);
        for i in 0..self.count as usize {
            let slot = (start + (i << log_bytes_in_oop)).into();
            #[cfg(not(feature = "debug_publish_object"))]
            closure.visit_slot(slot);
            #[cfg(feature = "debug_publish_object")]
            closure.visit_slot(_source, edge);
        }
    }
}

impl OopIterate for InstanceKlass {
    fn oop_iterate<const COMPRESSED: bool>(
        &self,
        oop: Oop,
        closure: &mut impl SlotVisitor<S<COMPRESSED>>,
    ) {
        let oop_maps = self.nonstatic_oop_maps();
        for map in oop_maps {
            map.oop_iterate::<COMPRESSED>(oop, closure)
        }
    }
}

impl OopIterate for InstanceMirrorKlass {
    fn oop_iterate<const COMPRESSED: bool>(
        &self,
        oop: Oop,
        closure: &mut impl SlotVisitor<S<COMPRESSED>>,
    ) {
        self.instance_klass.oop_iterate::<COMPRESSED>(oop, closure);
        #[cfg(feature = "debug_publish_object")]
        let _source = ObjectReference::from(oop);
        // static fields
        let start = Self::start_of_static_fields(oop);
        let len = Self::static_oop_field_count(oop);
        if COMPRESSED {
            let start: *const NarrowOop = start.to_ptr::<NarrowOop>();
            let slice = unsafe { slice::from_raw_parts(start, len as _) };
            for narrow_oop in slice {
                #[cfg(not(feature = "debug_publish_object"))]
                closure.visit_slot(narrow_oop.slot().into());
                #[cfg(feature = "debug_publish_object")]
                closure.visit_slot(_source, narrow_oop.slot().into());
            }
        } else {
            let start: *const Oop = start.to_ptr::<Oop>();
            let slice = unsafe { slice::from_raw_parts(start, len as _) };

            for oop in slice {
                #[cfg(not(feature = "debug_publish_object"))]
                closure.visit_slot(Address::from_ref(oop as &Oop).into());
                #[cfg(feature = "debug_publish_object")]
                closure.visit_slot(_source, Address::from_ref(oop as &Oop).into());
            }
        }
    }
}

impl OopIterate for InstanceClassLoaderKlass {
    fn oop_iterate<const COMPRESSED: bool>(
        &self,
        oop: Oop,
        closure: &mut impl SlotVisitor<S<COMPRESSED>>,
    ) {
        self.instance_klass.oop_iterate::<COMPRESSED>(oop, closure);
    }
}

impl OopIterate for ObjArrayKlass {
    fn oop_iterate<const COMPRESSED: bool>(
        &self,
        oop: Oop,
        closure: &mut impl SlotVisitor<S<COMPRESSED>>,
    ) {
        let array = unsafe { oop.as_array_oop() };
        #[cfg(feature = "debug_publish_object")]
        let _source = ObjectReference::from(oop);
        if COMPRESSED {
            for narrow_oop in unsafe { array.data::<NarrowOop, COMPRESSED>(BasicType::T_OBJECT) } {
                #[cfg(not(feature = "debug_publish_object"))]
                closure.visit_slot(narrow_oop.slot().into());
                #[cfg(feature = "debug_publish_object")]
                closure.visit_slot(_source, narrow_oop.slot().into());
            }
        } else {
            for oop in unsafe { array.data::<Oop, COMPRESSED>(BasicType::T_OBJECT) } {
                #[cfg(not(feature = "debug_publish_object"))]
                closure.visit_slot(Address::from_ref(oop as &Oop).into());
                #[cfg(feature = "debug_publish_object")]
                closure.visit_slot(_source, Address::from_ref(oop as &Oop).into());
            }
        }
    }
}

impl OopIterate for TypeArrayKlass {
    fn oop_iterate<const COMPRESSED: bool>(
        &self,
        _oop: Oop,
        _closure: &mut impl SlotVisitor<S<COMPRESSED>>,
    ) {
        // Performance tweak: We skip processing the klass pointer since all
        // TypeArrayKlasses are guaranteed processed via the null class loader.
    }
}

impl OopIterate for InstanceRefKlass {
    fn oop_iterate<const COMPRESSED: bool>(
        &self,
        oop: Oop,
        closure: &mut impl SlotVisitor<S<COMPRESSED>>,
    ) {
        use crate::abi::*;
        self.instance_klass.oop_iterate::<COMPRESSED>(oop, closure);

        let discovery_disabled = !closure.should_discover_references();

        if Self::should_discover_refs::<COMPRESSED>(
            self.instance_klass.reference_type,
            discovery_disabled,
        ) {
            // let reference = ObjectReference::from(oop);
            match self.instance_klass.reference_type {
                ReferenceType::None => {
                    panic!("oop_iterate on InstanceRefKlass with reference_type as None")
                }
                rt => {
                    if !Self::discover_reference::<COMPRESSED>(oop, rt) {
                        Self::process_ref_as_strong(oop, closure)
                    }
                }
            }
        } else {
            Self::process_ref_as_strong(oop, closure);
        }
    }
}

impl InstanceRefKlass {
    // fn should_scan_weak_refs<const COMPRESSED: bool>() -> bool {
    //     !*crate::singleton::<COMPRESSED>()
    //         .get_options()
    //         .no_reference_types
    // }

    fn should_discover_refs<const COMPRESSED: bool>(
        mut rt: ReferenceType,
        disable_discovery: bool,
    ) -> bool {
        if rt == ReferenceType::Other {
            rt = ReferenceType::Weak;
        }
        if disable_discovery {
            return false;
        }
        if *crate::singleton::<COMPRESSED>().get_options().no_finalizer
            && rt == ReferenceType::Final
        {
            return false;
        }
        if *crate::singleton::<COMPRESSED>()
            .get_options()
            .no_reference_types
            && rt != ReferenceType::Final
        {
            return false;
        }
        true
    }

    fn process_ref_as_strong<const COMPRESSED: bool>(
        oop: Oop,
        closure: &mut impl SlotVisitor<S<COMPRESSED>>,
    ) {
        #[cfg(feature = "debug_publish_object")]
        let _source = ObjectReference::from(oop);
        let referent_addr = Self::referent_address::<COMPRESSED>(oop);
        #[cfg(not(feature = "debug_publish_object"))]
        closure.visit_slot(referent_addr.into());
        #[cfg(feature = "debug_publish_object")]
        closure.visit_slot(_source, referent_addr.into());

        let discovered_addr = Self::discovered_address::<COMPRESSED>(oop);
        #[cfg(not(feature = "debug_publish_object"))]
        closure.visit_slot(discovered_addr.into());
        #[cfg(feature = "debug_publish_object")]
        closure.visit_slot(_source, discovered_addr.into());
    }

    fn discover_reference<const COMPRESSED: bool>(oop: Oop, rt: ReferenceType) -> bool {
        // Do not discover new refs during reference processing.
        if !DISCOVERED_LISTS.allow_discover() {
            return false;
        }
        // Do not discover if the referent is live.
        let addr = InstanceRefKlass::referent_address::<COMPRESSED>(oop);
        let Some(referent) = addr.load() else {
            return false;
        };
        // Skip live or null referents
        if referent.is_reachable::<OpenJDK<COMPRESSED>>() {
            return false;
        }
        // Skip young referents
        let reference: ObjectReference = oop.into();
        if !crate::singleton::<COMPRESSED>()
            .get_plan()
            .should_process_reference(reference, referent)
        {
            return false;
        }
        if rt == ReferenceType::Final && DISCOVERED_LISTS.is_discovered::<COMPRESSED>(reference) {
            return false;
        }
        // Add to reference list
        DISCOVERED_LISTS
            .get(rt)
            .add::<COMPRESSED>(reference, referent);
        true
    }
}

#[allow(unused)]
fn oop_iterate_slow<const COMPRESSED: bool, V: SlotVisitor<S<COMPRESSED>>>(
    oop: Oop,
    closure: &mut V,
    tls: OpaquePointer,
) {
    unsafe {
        CLOSURE.with(|x| *x.get() = closure as *mut V as *mut u8);
        ((*UPCALLS).scan_object)(
            mem::transmute(
                scan_object_fn::<COMPRESSED, V> as *const unsafe extern "C" fn(slot: Address),
            ),
            mem::transmute(oop),
            tls,
        );
    }
}

fn oop_iterate<const COMPRESSED: bool>(oop: Oop, closure: &mut impl SlotVisitor<S<COMPRESSED>>) {
    let klass = oop.klass::<COMPRESSED>();
    let klass_id = klass.id;
    assert!(
        klass_id as i32 >= 0 && (klass_id as i32) < 6,
        "Invalid klass-id: {:x} for oop: {:x}",
        klass_id as i32,
        unsafe { mem::transmute::<Oop, ObjectReference>(oop) }
    );
    match klass_id {
        KlassID::Instance => {
            let instance_klass = unsafe { klass.cast::<InstanceKlass>() };
            instance_klass.oop_iterate::<COMPRESSED>(oop, closure);
        }
        KlassID::InstanceClassLoader => {
            let instance_klass = unsafe { klass.cast::<InstanceClassLoaderKlass>() };
            instance_klass.oop_iterate::<COMPRESSED>(oop, closure);
        }
        KlassID::InstanceMirror => {
            let instance_klass = unsafe { klass.cast::<InstanceMirrorKlass>() };
            instance_klass.oop_iterate::<COMPRESSED>(oop, closure);
        }
        KlassID::ObjArray => {
            let array_klass = unsafe { klass.cast::<ObjArrayKlass>() };
            array_klass.oop_iterate::<COMPRESSED>(oop, closure);
        }
        KlassID::TypeArray => {
            // Skip scanning primitive arrays as they contain no reference fields.
        }
        KlassID::InstanceRef => {
            let instance_klass = unsafe { klass.cast::<InstanceRefKlass>() };
            instance_klass.oop_iterate::<COMPRESSED>(oop, closure);
        }
    }
}

thread_local! {
    static CLOSURE: UnsafeCell<*mut u8> = const { UnsafeCell::new(std::ptr::null_mut()) };
}

pub unsafe extern "C" fn scan_object_fn<
    const COMPRESSED: bool,
    V: SlotVisitor<OpenJDKSlot<COMPRESSED>>,
>(
    slot: Address,
) {
    let ptr: *mut u8 = CLOSURE.with(|x| *x.get());
    let closure = &mut *(ptr as *mut V);
    #[cfg(feature = "debug_publish_object")]
    closure.visit_slot(edge.load(), edge.into());
    #[cfg(not(feature = "debug_publish_object"))]
    closure.visit_slot(slot.into());
}

pub fn scan_object<const COMPRESSED: bool>(
    object: ObjectReference,
    closure: &mut impl SlotVisitor<S<COMPRESSED>>,
    _tls: VMWorkerThread,
) {
    unsafe { oop_iterate::<COMPRESSED>(mem::transmute(object), closure) }
}
