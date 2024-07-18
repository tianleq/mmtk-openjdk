use crate::abi::Oop;
use crate::UPCALLS;
use crate::{vm_metadata, OpenJDK};
use mmtk::util::alloc::fill_alignment_gap;
use mmtk::util::copy::*;
use mmtk::util::{Address, ObjectReference};
use mmtk::vm::*;

pub struct VMObjectModel<const COMPRESSED: bool> {}

impl<const COMPRESSED: bool> ObjectModel<OpenJDK<COMPRESSED>> for VMObjectModel<COMPRESSED> {
    const GLOBAL_LOG_BIT_SPEC: VMGlobalLogBitSpec = vm_metadata::LOGGING_SIDE_METADATA_SPEC;

    const LOCAL_FORWARDING_POINTER_SPEC: VMLocalForwardingPointerSpec =
        vm_metadata::FORWARDING_POINTER_METADATA_SPEC;
    const LOCAL_FORWARDING_BITS_SPEC: VMLocalForwardingBitsSpec =
        vm_metadata::FORWARDING_BITS_METADATA_SPEC;
    const LOCAL_MARK_BIT_SPEC: VMLocalMarkBitSpec = vm_metadata::MARKING_METADATA_SPEC;
    const LOCAL_LOS_MARK_NURSERY_SPEC: VMLocalLOSMarkNurserySpec = vm_metadata::LOS_METADATA_SPEC;

    const UNIFIED_OBJECT_REFERENCE_ADDRESS: bool = true;
    const OBJECT_REF_OFFSET_LOWER_BOUND: isize = 0;

    fn copy(
        from: ObjectReference,
        copy: CopySemantics,
        copy_context: &mut GCWorkerCopyContext<OpenJDK<COMPRESSED>>,
    ) -> ObjectReference {
        let bytes = unsafe { Oop::from(from).size::<COMPRESSED>() };
        let dst = copy_context.alloc_copy(from, bytes, ::std::mem::size_of::<usize>(), 0, copy);
        debug_assert!(!dst.is_zero());
        // Copy
        let src = from.to_raw_address();

        // {
        //     for i in 0..(bytes >> 3) {
        //         let p = dst.add(i << 3).to_ptr();
        //         assert!(
        //             0 == unsafe { std::ptr::read::<usize>(p) },
        //             "{:?} overwritten",
        //             dst
        //         );
        //     }
        // }

        unsafe { std::ptr::copy_nonoverlapping::<u8>(src.to_ptr(), dst.to_mut_ptr(), bytes) }
        // Note on onsafe: `alloc_copy` never returns 0.
        let to_obj = unsafe { ObjectReference::from_raw_address_unchecked(dst) };
        copy_context.post_copy(to_obj, bytes, copy);
        to_obj
    }

    #[cfg(feature = "thread_local_gc_copying")]
    /// Copy an object and return the address of the new object. This is executed by mutator iteself
    /// and therefore is using mutator's allocator
    /// Arguments:
    /// * `from`: The address of the object to be copied.
    /// * `semantics`: The copy semantic to use.
    /// * `mutator`: The `Mutator` for the mutator thread.
    fn thread_local_copy(
        from: ObjectReference,
        _semantics: CopySemantics,
        mutator: &mut crate::Mutator<OpenJDK<COMPRESSED>>,
    ) -> ObjectReference {
        use mmtk::MutatorContext;

        let bytes = unsafe { Oop::from(from).size::<COMPRESSED>() };
        let dst = mutator.alloc_copy(bytes, ::std::mem::size_of::<usize>(), 0);
        // Copy
        let src = from.to_raw_address();
        unsafe { std::ptr::copy_nonoverlapping::<u8>(src.to_ptr(), dst.to_mut_ptr(), bytes) }
        let to_obj = ObjectReference::from_raw_address(dst);
        mutator.post_copy(to_obj, bytes);
        to_obj
    }

    fn copy_to(from: ObjectReference, to: ObjectReference, region: Address) -> Address {
        let need_copy = from != to;
        let bytes = unsafe { Oop::from(from).size::<COMPRESSED>() };
        if need_copy {
            // copy obj to target
            let dst = to.to_raw_address();
            // Copy
            let src = from.to_raw_address();
            for i in 0..bytes {
                unsafe { (dst + i).store((src + i).load::<u8>()) };
            }
        }
        let start = Self::ref_to_object_start(to);
        if region != Address::ZERO {
            fill_alignment_gap::<OpenJDK<COMPRESSED>>(region, start);
        }
        start + bytes
    }

    fn get_reference_when_copied_to(_from: ObjectReference, to: Address) -> ObjectReference {
        debug_assert!(!to.is_zero());
        unsafe { ObjectReference::from_raw_address_unchecked(to) }
    }

    fn get_current_size(object: ObjectReference) -> usize {
        unsafe { Oop::from(object).size::<COMPRESSED>() }
    }

    fn get_size_when_copied(object: ObjectReference) -> usize {
        Self::get_current_size(object)
    }

    fn get_align_when_copied(_object: ObjectReference) -> usize {
        // FIXME figure out the proper alignment
        ::std::mem::size_of::<usize>()
    }

    fn get_align_offset_when_copied(_object: ObjectReference) -> usize {
        0
    }

    fn get_type_descriptor(_reference: ObjectReference) -> &'static [i8] {
        unimplemented!()
    }

    fn ref_to_object_start(object: ObjectReference) -> Address {
        object.to_raw_address()
    }

    fn ref_to_header(object: ObjectReference) -> Address {
        object.to_raw_address()
    }

    const IN_OBJECT_ADDRESS_OFFSET: isize = 0;

    fn dump_object(object: ObjectReference) {
        unsafe {
            ((*UPCALLS).dump_object)(object);
        }
    }

    fn is_object_sane(object: ObjectReference) -> bool {
        let oop = Oop::from(object);
        // It is only valid if klass.id is between 0 and 5 (see KlassID in openjdk/src/hotspot/share/oops/klass.hpp)
        // If oop.klass is not a valid pointer, we may segfault here.
        let klass_id = oop.klass::<COMPRESSED>().id as i32;
        (0..6).contains(&klass_id)
    }

    fn null_slot() -> crate::OpenJDKEdge<COMPRESSED> {
        crate::OpenJDKEdge::from(Address::ZERO)
    }
}
