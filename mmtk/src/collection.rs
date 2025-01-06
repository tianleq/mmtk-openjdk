#[cfg(feature = "thread_local_gc")]
use crate::OpenJDKSlot;
use crate::UPCALLS;
use crate::{MutatorClosure, OpenJDK};
use mmtk::util::alloc::AllocationError;
use mmtk::util::opaque_pointer::*;
#[cfg(feature = "thread_local_gc")]
use mmtk::vm::ObjectGraphTraversal;
use mmtk::vm::{Collection, GCThreadContext};
use mmtk::Mutator;

pub struct VMCollection {}

const GC_THREAD_KIND_WORKER: libc::c_int = 1;

impl<const COMPRESSED: bool> Collection<OpenJDK<COMPRESSED>> for VMCollection {
    fn stop_all_mutators<F>(tls: VMWorkerThread, mut mutator_visitor: F)
    where
        F: FnMut(&'static mut Mutator<OpenJDK<COMPRESSED>>),
    {
        unsafe {
            ((*UPCALLS).stop_all_mutators)(
                tls,
                MutatorClosure::from_rust_closure::<_, COMPRESSED>(&mut mutator_visitor),
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
        mut object_graph_traversal_func: impl ObjectGraphTraversal<OpenJDKSlot<COMPRESSED>>,
    ) {
        unsafe {
            ((*UPCALLS).scan_roots_in_mutator_thread)(
                crate::scanning::to_thread_local_graph_traversal_closure::<
                    OpenJDKSlot<COMPRESSED>,
                    _,
                >(&mut object_graph_traversal_func),
                tls,
            );
        }
    }

    fn spawn_gc_thread(tls: VMThread, ctx: GCThreadContext<OpenJDK<COMPRESSED>>) {
        let (ctx_ptr, kind) = match ctx {
            GCThreadContext::Worker(w) => {
                (Box::into_raw(w) as *mut libc::c_void, GC_THREAD_KIND_WORKER)
            }
        };
        unsafe {
            ((*UPCALLS).spawn_gc_thread)(tls, kind, ctx_ptr);
        }
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

    fn do_thread_local_gc(tls: VMMutatorThread) {
        unsafe { ((*UPCALLS).execute_thread_local_gc)(tls) }
    }
}
