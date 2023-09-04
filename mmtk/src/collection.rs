use crate::SINGLETON;
use crate::UPCALLS;
use crate::{MutatorClosure, OpenJDK};
use mmtk::memory_manager;
use mmtk::util::alloc::AllocationError;
use mmtk::util::opaque_pointer::*;
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

    fn scan_mutator<F>(tls: VMMutatorThread, mut mutator_visitor: F)
    where
        F: FnMut(&'static mut Mutator<OpenJDK>),
    {
        let scan_mutators_in_safepoint =
            <OpenJDK as VMBinding>::VMScanning::SCAN_MUTATORS_IN_SAFEPOINT;

        unsafe {
            ((*UPCALLS).scan_mutator)(
                tls,
                scan_mutators_in_safepoint,
                MutatorClosure::from_rust_closure(&mut mutator_visitor),
            );
        }
    }

    fn block_for_thread_local_gc(_tls: VMMutatorThread) {
        unsafe {
            ((*UPCALLS).block_for_thread_local_gc)();
        }
    }

    fn resume_from_thread_local_gc(tls: VMMutatorThread) {
        unsafe {
            ((*UPCALLS).resume_from_thread_local_gc)(tls);
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

    fn publish_vm_specific_roots(mutator: &'static mut Mutator<OpenJDK>) {
        use mmtk::vm::edge_shape::Edge;
        for roots in (*crate::CODE_CACHE_ROOTS.lock().unwrap()).values() {
            for r in roots {
                memory_manager::mmtk_publish_object::<OpenJDK>(
                    &SINGLETON,
                    Edge::load(r),
                    Some(mutator.mutator_id),
                );
            }
        }
    }
}
