use crate::gc_work::*;
use crate::Slot;
use crate::{NewBuffer, OpenJDKSlot, UPCALLS};
use crate::{OpenJDK, SlotsClosure};
use mmtk::memory_manager;
use mmtk::scheduler::WorkBucketStage;
use mmtk::util::opaque_pointer::*;
use mmtk::util::{Address, ObjectReference};
use mmtk::vm::ObjectGraphTraversal;
use mmtk::vm::{RootsWorkFactory, Scanning, SlotVisitor};
use mmtk::Mutator;
use mmtk::MutatorContext;

pub struct VMScanning {}

pub(crate) const WORK_PACKET_CAPACITY: usize = mmtk::scheduler::EDGES_WORK_BUFFER_SIZE;

extern "C" fn report_slots_and_renew_buffer<S: Slot, F: RootsWorkFactory<S>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    factory_ptr: *mut libc::c_void,
) -> NewBuffer {
    if !ptr.is_null() {
        // Note: Currently OpenJDKSlot has the same layout as Address.  If the layout changes, we
        // should fix the Rust-to-C interface.
        let buf = unsafe { Vec::<S>::from_raw_parts(ptr as _, length, capacity) };
        let factory: &mut F = unsafe { &mut *(factory_ptr as *mut F) };
        factory.create_process_roots_work(buf);
    }
    let (ptr, _, capacity) = {
        // TODO: Use Vec::into_raw_parts() when the method is available.
        use std::mem::ManuallyDrop;
        let new_vec = Vec::with_capacity(WORK_PACKET_CAPACITY);
        let mut me = ManuallyDrop::new(new_vec);
        (me.as_mut_ptr(), me.len(), me.capacity())
    };
    NewBuffer { ptr, capacity }
}

pub(crate) fn to_slots_closure<S: Slot, F: RootsWorkFactory<S>>(factory: &mut F) -> SlotsClosure {
    SlotsClosure {
        func: report_slots_and_renew_buffer::<S, F>,
        data: factory as *mut F as *mut libc::c_void,
    }
}

extern "C" fn object_graph_traversal_report_roots<S: Slot, C: ObjectGraphTraversal<S>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    traverse_func: *mut libc::c_void,
) -> NewBuffer {
    if !ptr.is_null() {
        let buf = unsafe { Vec::<S>::from_raw_parts(ptr as _, length, capacity) };
        let traverse_func: &mut C = unsafe { &mut *(traverse_func as *mut C) };
        traverse_func.report_roots(buf);
    }

    let (ptr, _, capacity) = {
        // TODO: Use Vec::into_raw_parts() when the method is available.
        use std::mem::ManuallyDrop;
        let new_vec = Vec::with_capacity(WORK_PACKET_CAPACITY);
        let mut me = ManuallyDrop::new(new_vec);
        (me.as_mut_ptr(), me.len(), me.capacity())
    };

    NewBuffer { ptr, capacity }
}

pub(crate) fn object_graph_traversal_to_slots_closure<S: Slot, C: ObjectGraphTraversal<S>>(
    closure: &mut C,
) -> SlotsClosure {
    SlotsClosure {
        func: object_graph_traversal_report_roots::<S, C>,
        data: closure as *mut C as *mut libc::c_void,
    }
}

impl<const COMPRESSED: bool> Scanning<OpenJDK<COMPRESSED>> for VMScanning {
    fn scan_object<SV: SlotVisitor<OpenJDKSlot<COMPRESSED>>>(
        tls: VMWorkerThread,
        object: ObjectReference,
        slot_visitor: &mut SV,
    ) {
        crate::object_scanning::scan_object::<COMPRESSED>(object, slot_visitor, tls);
    }

    fn notify_initial_thread_scan_complete(_partial_scan: bool, _tls: VMWorkerThread) {
        // unimplemented!()
        // TODO
    }

    fn scan_roots_in_mutator_thread(
        _tls: VMWorkerThread,
        mutator: &'static mut Mutator<OpenJDK<COMPRESSED>>,
        mut factory: impl RootsWorkFactory<OpenJDKSlot<COMPRESSED>>,
    ) {
        let tls = mutator.get_tls();
        unsafe {
            ((*UPCALLS).scan_roots_in_mutator_thread)(to_slots_closure(&mut factory), tls);
        }
    }

    fn scan_vm_specific_roots(
        _tls: VMWorkerThread,
        factory: impl RootsWorkFactory<OpenJDKSlot<COMPRESSED>>,
    ) {
        memory_manager::add_work_packets(
            crate::singleton::<COMPRESSED>(),
            WorkBucketStage::Prepare,
            vec![
                Box::new(ScanUniverseRoots::new(factory.clone())) as _,
                Box::new(ScanJNIHandlesRoots::new(factory.clone())) as _,
                Box::new(ScanObjectSynchronizerRoots::new(factory.clone())) as _,
                Box::new(ScanManagementRoots::new(factory.clone())) as _,
                Box::new(ScanJvmtiExportRoots::new(factory.clone())) as _,
                Box::new(ScanAOTLoaderRoots::new(factory.clone())) as _,
                Box::new(ScanSystemDictionaryRoots::new(factory.clone())) as _,
                Box::new(ScanCodeCacheRoots::new(factory.clone())) as _,
                Box::new(ScanStringTableRoots::new(factory.clone())) as _,
                Box::new(ScanClassLoaderDataGraphRoots::new(factory.clone())) as _,
                Box::new(ScanWeakProcessorRoots::new(factory.clone())) as _,
                Box::new(ScanVMThreadRoots::new(factory)) as _,
            ],
        );
    }

    fn supports_return_barrier() -> bool {
        unimplemented!()
    }

    fn prepare_for_roots_re_scanning() {
        unsafe {
            ((*UPCALLS).prepare_for_roots_re_scanning)();
        }
    }

    fn single_threaded_scan_vm_specific_roots(
        _tls: VMWorkerThread,

        mut closure: impl ObjectGraphTraversal<OpenJDKSlot<COMPRESSED>>,
    ) {
        unsafe {
            ((*UPCALLS).scan_universe_roots)(object_graph_traversal_to_slots_closure(&mut closure));
            ((*UPCALLS).scan_jni_handle_roots)(object_graph_traversal_to_slots_closure(
                &mut closure,
            ));
            ((*UPCALLS).scan_object_synchronizer_roots)(object_graph_traversal_to_slots_closure(
                &mut closure,
            ));
            ((*UPCALLS).scan_management_roots)(object_graph_traversal_to_slots_closure(
                &mut closure,
            ));
            ((*UPCALLS).scan_jvmti_export_roots)(object_graph_traversal_to_slots_closure(
                &mut closure,
            ));
            ((*UPCALLS).scan_aot_loader_roots)(object_graph_traversal_to_slots_closure(
                &mut closure,
            ));
            ((*UPCALLS).scan_system_dictionary_roots)(object_graph_traversal_to_slots_closure(
                &mut closure,
            ));
            ((*UPCALLS).scan_string_table_roots)(object_graph_traversal_to_slots_closure(
                &mut closure,
            ));
            ((*UPCALLS).scan_class_loader_data_graph_roots)(
                object_graph_traversal_to_slots_closure(&mut closure),
            );
            ((*UPCALLS).scan_weak_processor_roots)(object_graph_traversal_to_slots_closure(
                &mut closure,
            ));
            ((*UPCALLS).scan_vm_thread_roots)(object_graph_traversal_to_slots_closure(
                &mut closure,
            ));
        }

        {
            let is_current_gc_nursery = crate::singleton::<COMPRESSED>()
                .get_plan()
                .generational()
                .is_some_and(|gen| gen.is_current_gc_nursery());

            let mut slots = Vec::with_capacity(WORK_PACKET_CAPACITY);
            let mut nursery_slots = 0;
            let mut mature_slots = 0;
            let mut add_roots = |roots: &[Address]| {
                for root in roots {
                    slots.push(OpenJDKSlot::<COMPRESSED>::from(*root));
                    if slots.len() >= WORK_PACKET_CAPACITY {
                        closure.report_roots(std::mem::take(&mut slots));
                    }
                }
            };

            {
                let mut mature = crate::MATURE_CODE_CACHE_ROOTS.lock().unwrap();
                // Only scan mature roots in full-heap collections.
                if !is_current_gc_nursery {
                    for roots in mature.values() {
                        mature_slots += roots.len();
                        add_roots(roots);
                    }
                }
                {
                    let mut nursery = crate::NURSERY_CODE_CACHE_ROOTS.lock().unwrap();

                    for (key, roots) in nursery.drain() {
                        nursery_slots += roots.len();
                        add_roots(&roots);
                        mature.insert(key, roots);
                    }
                }
            }
            probe!(mmtk_openjdk, code_cache_roots, nursery_slots, mature_slots);
            if !slots.is_empty() {
                closure.report_roots(slots);
            }
        }
    }

    fn single_threaded_scan_roots_in_mutator_thread(
        _tls: VMWorkerThread,
        mutator: &'static mut Mutator<OpenJDK<COMPRESSED>>,
        mut closure: impl ObjectGraphTraversal<OpenJDKSlot<COMPRESSED>>,
    ) {
        let tls = mutator.get_tls();

        unsafe {
            ((*UPCALLS).scan_roots_in_mutator_thread)(
                object_graph_traversal_to_slots_closure(&mut closure),
                tls,
            );
        }
    }
}
