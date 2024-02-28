#[cfg(feature = "thread_local_gc")]
use crate::OpenJDKEdge;
use crate::UPCALLS;
use crate::{MutatorClosure, OpenJDK};
use mmtk::util::alloc::AllocationError;
use mmtk::util::opaque_pointer::*;
#[cfg(feature = "thread_local_gc")]
use mmtk::vm::ObjectGraphTraversal;
use mmtk::vm::{Collection, GCThreadContext, Scanning, VMBinding};
use mmtk::{Mutator, MutatorContext};

pub struct VMCollection {}

const GC_THREAD_KIND_CONTROLLER: libc::c_int = 0;
const GC_THREAD_KIND_WORKER: libc::c_int = 1;

impl Collection<OpenJDK> for VMCollection {
    fn stop_all_mutators<F>(tls: VMWorkerThread, mut mutator_visitor: F)
    where
        F: FnMut(&'static mut Mutator<OpenJDK>),
    {
        let scan_mutators_in_safepoint =
            <OpenJDK as VMBinding>::VMScanning::SCAN_MUTATORS_IN_SAFEPOINT;

        unsafe {
            ((*UPCALLS).stop_all_mutators)(
                tls,
                scan_mutators_in_safepoint,
                MutatorClosure::from_rust_closure(&mut mutator_visitor),
            );
        }
    }

    fn resume_mutators(tls: VMWorkerThread) {
        unsafe {
            ((*UPCALLS).resume_mutators)(tls);
        }
    }

    fn block_for_gc(_tls: VMMutatorThread) {
        unsafe {
            ((*UPCALLS).block_for_gc)();
        }
    }
    #[cfg(feature = "thread_local_gc")]
    fn scan_mutator(
        tls: VMMutatorThread,
        mut object_graph_traversal_func: impl ObjectGraphTraversal<OpenJDKEdge>,
    ) {
        unsafe {
            ((*UPCALLS).scan_roots_in_mutator_thread)(
                crate::scanning::to_thread_local_graph_traversal_closure(
                    &mut object_graph_traversal_func,
                ),
                tls,
            );
        }
    }

    fn spawn_gc_thread(tls: VMThread, ctx: GCThreadContext<OpenJDK>) {
        let (ctx_ptr, kind) = match ctx {
            GCThreadContext::Controller(c) => (
                Box::into_raw(c) as *mut libc::c_void,
                GC_THREAD_KIND_CONTROLLER,
            ),
            GCThreadContext::Worker(w) => {
                (Box::into_raw(w) as *mut libc::c_void, GC_THREAD_KIND_WORKER)
            }
        };
        unsafe {
            ((*UPCALLS).spawn_gc_thread)(tls, kind, ctx_ptr);
        }
    }

    fn prepare_mutator<T: MutatorContext<OpenJDK>>(
        _tls_w: VMWorkerThread,
        _tls_m: VMMutatorThread,
        _m: &T,
    ) {
        // unimplemented!()
    }

    fn out_of_memory(tls: VMThread, err_kind: AllocationError) {
        unsafe {
            ((*UPCALLS).out_of_memory)(tls, err_kind);
        }
    }

    fn schedule_finalization(_tls: VMWorkerThread) {
        unsafe {
            ((*UPCALLS).schedule_finalizer)();
        }
    }
}
