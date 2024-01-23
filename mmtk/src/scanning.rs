use crate::gc_work::*;
use crate::{EdgesClosure, OpenJDK};
use crate::{NewBuffer, OpenJDKEdge, SINGLETON, UPCALLS};
use mmtk::memory_manager;
use mmtk::scheduler::WorkBucketStage;
use mmtk::util::opaque_pointer::*;
use mmtk::util::{Address, ObjectReference};
#[cfg(feature = "thread_local_gc")]
use mmtk::vm::ObjectGraphTraversal;
use mmtk::vm::{EdgeVisitor, RootsWorkFactory, Scanning};
use mmtk::Mutator;
use mmtk::MutatorContext;

pub struct VMScanning {}

const WORK_PACKET_CAPACITY: usize = 4096;

extern "C" fn report_edges_and_renew_buffer<F: RootsWorkFactory<OpenJDKEdge>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    factory_ptr: *mut libc::c_void,
    vm_roots_type: u8,
) -> NewBuffer {
    if !ptr.is_null() {
        let buf = unsafe { Vec::<Address>::from_raw_parts(ptr, length, capacity) };
        let factory: &mut F = unsafe { &mut *(factory_ptr as *mut F) };
        factory.create_process_edge_roots_work(vm_roots_type, buf);
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

#[cfg(feature = "thread_local_gc")]
extern "C" fn traverse_thread_local_object_graph<F: ObjectGraphTraversal<OpenJDKEdge>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    traverse_func: *mut libc::c_void,
    _vm_roots_type: u8,
) -> NewBuffer {
    if !ptr.is_null() {
        let buf = unsafe { Vec::<Address>::from_raw_parts(ptr, length, capacity) };
        let traverse_func: &mut F = unsafe { &mut *(traverse_func as *mut F) };
        traverse_func.traverse_from_roots(buf);
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

pub(crate) fn to_edges_closure<F: RootsWorkFactory<OpenJDKEdge>>(factory: &mut F) -> EdgesClosure {
    EdgesClosure {
        func: report_edges_and_renew_buffer::<F>,
        data: factory as *mut F as *mut libc::c_void,
    }
}

#[cfg(feature = "thread_local_gc")]
pub(crate) fn to_thread_local_graph_traversal_closure<F: ObjectGraphTraversal<OpenJDKEdge>>(
    graph_traversal_func: &mut F,
) -> EdgesClosure {
    EdgesClosure {
        func: traverse_thread_local_object_graph::<F>,
        data: graph_traversal_func as *mut F as *mut libc::c_void,
    }
}

impl Scanning<OpenJDK> for VMScanning {
    const SCAN_MUTATORS_IN_SAFEPOINT: bool = false;
    const SINGLE_THREAD_MUTATOR_SCANNING: bool = false;

    fn scan_object<EV: EdgeVisitor<OpenJDKEdge>>(
        tls: VMWorkerThread,
        object: ObjectReference,
        edge_visitor: &mut EV,
    ) {
        crate::object_scanning::scan_object(object, edge_visitor, tls)
    }

    fn notify_initial_thread_scan_complete(_partial_scan: bool, _tls: VMWorkerThread) {
        // unimplemented!()
        // TODO
    }

    fn scan_roots_in_all_mutator_threads(
        _tls: VMWorkerThread,
        mut factory: impl RootsWorkFactory<OpenJDKEdge>,
    ) {
        unsafe {
            ((*UPCALLS).scan_roots_in_all_mutator_threads)(to_edges_closure(&mut factory));
        }
    }

    fn scan_roots_in_mutator_thread(
        _tls: VMWorkerThread,
        mutator: &'static mut Mutator<OpenJDK>,
        mut factory: impl RootsWorkFactory<OpenJDKEdge>,
    ) {
        let tls = mutator.get_tls();
        unsafe {
            ((*UPCALLS).scan_roots_in_mutator_thread)(to_edges_closure(&mut factory), tls);
        }
    }

    // #[cfg(feature = "thread_local_gc")]
    // fn thread_local_scan_roots_of_mutator_threads(
    //     _tls: VMWorkerThread,
    //     mutator: &'static mut Mutator<OpenJDK>,
    //     mut factory: impl RootsWorkFactory<OpenJDKEdge>,
    // ) {
    //     let tls = mutator.get_tls();
    //     unsafe {
    //         ((*UPCALLS).thread_local_scan_roots_of_mutator_threads)(
    //             to_edges_closure(&mut factory),
    //             tls,
    //         );
    //     }
    // }

    fn scan_vm_specific_roots(_tls: VMWorkerThread, factory: impl RootsWorkFactory<OpenJDKEdge>) {
        memory_manager::add_work_packets(
            &SINGLETON,
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
            ],
        );
        if !(Self::SCAN_MUTATORS_IN_SAFEPOINT && Self::SINGLE_THREAD_MUTATOR_SCANNING) {
            memory_manager::add_work_packet(
                &SINGLETON,
                WorkBucketStage::Prepare,
                ScanVMThreadRoots::new(factory),
            );
        }
    }

    fn single_thread_scan_vm_specific_roots(
        _tls: VMWorkerThread,
        factory: impl RootsWorkFactory<OpenJDKEdge>,
    ) {
        memory_manager::add_local_work_packets(
            &SINGLETON,
            WorkBucketStage::Unconstrained,
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
            ],
        );
        if !(Self::SCAN_MUTATORS_IN_SAFEPOINT && Self::SINGLE_THREAD_MUTATOR_SCANNING) {
            memory_manager::add_local_work_packet(
                &SINGLETON,
                WorkBucketStage::Unconstrained,
                ScanVMThreadRoots::new(factory),
            );
        }
    }

    fn supports_return_barrier() -> bool {
        unimplemented!()
    }

    fn prepare_for_roots_re_scanning() {
        unsafe {
            ((*UPCALLS).prepare_for_roots_re_scanning)();
        }
    }
}
