use crate::NewBuffer;
use crate::OpenJDK;
use crate::OpenJDK_Upcalls;
use crate::BUILDER;
use crate::SINGLETON;
use crate::UPCALLS;
use libc::c_char;
use mmtk::memory_manager;
use mmtk::plan::BarrierSelector;
use mmtk::scheduler::GCController;
use mmtk::scheduler::GCWorker;
use mmtk::util::alloc::AllocatorSelector;
use mmtk::util::constants::LOG_BYTES_IN_ADDRESS;
use mmtk::util::opaque_pointer::*;
use mmtk::util::{Address, ObjectReference};
use mmtk::AllocationSemantics;
use mmtk::Mutator;
use mmtk::MutatorContext;
use once_cell::sync;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::sync::atomic::Ordering;

// Supported barriers:
static NO_BARRIER: sync::Lazy<CString> = sync::Lazy::new(|| CString::new("NoBarrier").unwrap());
static OBJECT_BARRIER: sync::Lazy<CString> =
    sync::Lazy::new(|| CString::new("ObjectBarrier").unwrap());
static OBJECT_OWNER_BARRIER: sync::Lazy<CString> =
    sync::Lazy::new(|| CString::new("ObjectOwnerBarrier").unwrap());

#[no_mangle]
pub extern "C" fn get_mmtk_version() -> *const c_char {
    crate::build_info::MMTK_OPENJDK_FULL_VERSION.as_ptr() as _
}

#[no_mangle]
pub extern "C" fn mmtk_active_barrier() -> *const c_char {
    match SINGLETON.get_plan().constraints().barrier {
        BarrierSelector::NoBarrier => NO_BARRIER.as_ptr(),
        BarrierSelector::ObjectBarrier => OBJECT_BARRIER.as_ptr(),
        BarrierSelector::ObjectOwnerBarrier => OBJECT_OWNER_BARRIER.as_ptr(),
        // In case we have more barriers in mmtk-core.
        #[allow(unreachable_patterns)]
        _ => unimplemented!(),
    }
}

/// # Safety
/// Caller needs to make sure the ptr is a valid vector pointer.
#[no_mangle]
pub unsafe extern "C" fn release_buffer(ptr: *mut Address, length: usize, capacity: usize) {
    let _vec = Vec::<Address>::from_raw_parts(ptr, length, capacity);
}

#[no_mangle]
pub extern "C" fn openjdk_gc_init(calls: *const OpenJDK_Upcalls) {
    unsafe { UPCALLS = calls };
    crate::abi::validate_memory_layouts();

    // We don't really need this, as we can dynamically set plans. However, for compatability of our CI scripts,
    // we allow selecting a plan using feature at build time.
    // We should be able to remove this very soon.
    {
        use mmtk::util::options::PlanSelector;
        let force_plan = if cfg!(feature = "nogc") {
            Some(PlanSelector::NoGC)
        } else if cfg!(feature = "semispace") {
            Some(PlanSelector::SemiSpace)
        } else if cfg!(feature = "gencopy") {
            Some(PlanSelector::GenCopy)
        } else if cfg!(feature = "marksweep") {
            Some(PlanSelector::MarkSweep)
        } else if cfg!(feature = "markcompact") {
            Some(PlanSelector::MarkCompact)
        } else if cfg!(feature = "pageprotect") {
            Some(PlanSelector::PageProtect)
        } else if cfg!(feature = "immix") {
            Some(PlanSelector::Immix)
        } else {
            None
        };
        if let Some(plan) = force_plan {
            BUILDER.lock().unwrap().options.plan.set(plan);
        }
    }

    // Make sure that we haven't initialized MMTk (by accident) yet
    assert!(!crate::MMTK_INITIALIZED.load(Ordering::SeqCst));
    // Make sure we initialize MMTk here
    lazy_static::initialize(&SINGLETON);
}

#[no_mangle]
pub extern "C" fn openjdk_is_gc_initialized() -> bool {
    crate::MMTK_INITIALIZED.load(std::sync::atomic::Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn mmtk_set_heap_size(size: usize) -> bool {
    let mut builder = BUILDER.lock().unwrap();
    builder.options.heap_size.set(size)
}

#[no_mangle]
pub extern "C" fn bind_mutator(tls: VMMutatorThread) -> *mut Mutator<OpenJDK> {
    Box::into_raw(memory_manager::bind_mutator(&SINGLETON, tls))
}

#[no_mangle]
// It is fine we turn the pointer back to box, as we turned a boxed value to the raw pointer in bind_mutator()
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn destroy_mutator(mutator: *mut Mutator<OpenJDK>) {
    memory_manager::destroy_mutator(unsafe { Box::from_raw(mutator) })
}

#[no_mangle]
// We trust the mutator pointer is valid.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn flush_mutator(mutator: *mut Mutator<OpenJDK>) {
    memory_manager::flush_mutator(unsafe { &mut *mutator })
}

#[no_mangle]
// We trust the mutator pointer is valid.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn alloc(
    mutator: *mut Mutator<OpenJDK>,
    size: usize,
    align: usize,
    offset: isize,
    allocator: AllocationSemantics,
) -> Address {
    memory_manager::alloc::<OpenJDK>(unsafe { &mut *mutator }, size, align, offset, allocator)
}

#[no_mangle]
pub extern "C" fn get_allocator_mapping(allocator: AllocationSemantics) -> AllocatorSelector {
    memory_manager::get_allocator_mapping(&SINGLETON, allocator)
}

#[no_mangle]
pub extern "C" fn get_max_non_los_default_alloc_bytes() -> usize {
    SINGLETON
        .get_plan()
        .constraints()
        .max_non_los_default_alloc_bytes
}

#[no_mangle]
// We trust the mutator pointer is valid.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn post_alloc(
    mutator: *mut Mutator<OpenJDK>,
    refer: ObjectReference,
    bytes: usize,
    allocator: AllocationSemantics,
) {
    memory_manager::post_alloc::<OpenJDK>(unsafe { &mut *mutator }, refer, bytes, allocator)
}

#[no_mangle]
pub extern "C" fn will_never_move(object: ObjectReference) -> bool {
    !object.is_movable()
}

#[no_mangle]
// We trust the gc_collector pointer is valid.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn start_control_collector(
    tls: VMWorkerThread,
    gc_controller: *mut GCController<OpenJDK>,
) {
    let mut gc_controller = unsafe { Box::from_raw(gc_controller) };
    memory_manager::start_control_collector(&SINGLETON, tls, &mut gc_controller);
}

#[no_mangle]
// We trust the worker pointer is valid.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn start_worker(tls: VMWorkerThread, worker: *mut GCWorker<OpenJDK>) {
    let mut worker = unsafe { Box::from_raw(worker) };
    memory_manager::start_worker::<OpenJDK>(&SINGLETON, tls, &mut worker)
}

#[no_mangle]
pub extern "C" fn initialize_collection(tls: VMThread) {
    memory_manager::initialize_collection(&SINGLETON, tls)
}

#[no_mangle]
pub extern "C" fn used_bytes() -> usize {
    memory_manager::used_bytes(&SINGLETON)
}

#[no_mangle]
pub extern "C" fn free_bytes() -> usize {
    memory_manager::free_bytes(&SINGLETON)
}

#[no_mangle]
pub extern "C" fn total_bytes() -> usize {
    memory_manager::total_bytes(&SINGLETON)
}

#[no_mangle]
#[cfg(feature = "sanity")]
pub extern "C" fn scan_region() {
    memory_manager::scan_region(&SINGLETON)
}

#[no_mangle]
pub extern "C" fn handle_user_collection_request(tls: VMMutatorThread) {
    memory_manager::handle_user_collection_request::<OpenJDK>(&SINGLETON, tls);
}

#[no_mangle]
pub extern "C" fn is_in_mmtk_spaces(object: ObjectReference) -> bool {
    memory_manager::is_in_mmtk_spaces(object)
}

#[no_mangle]
pub extern "C" fn is_mapped_address(addr: Address) -> bool {
    memory_manager::is_mapped_address(addr)
}

#[no_mangle]
pub extern "C" fn modify_check(object: ObjectReference) {
    memory_manager::modify_check(&SINGLETON, object)
}

#[no_mangle]
pub extern "C" fn add_weak_candidate(reff: ObjectReference) {
    memory_manager::add_weak_candidate(&SINGLETON, reff)
}

#[no_mangle]
pub extern "C" fn add_soft_candidate(reff: ObjectReference) {
    memory_manager::add_soft_candidate(&SINGLETON, reff)
}

#[no_mangle]
pub extern "C" fn add_phantom_candidate(reff: ObjectReference) {
    memory_manager::add_phantom_candidate(&SINGLETON, reff)
}

// The harness_begin()/end() functions are different than other API functions in terms of the thread state.
// Other functions are called by the VM, thus the thread should already be in the VM state. But the harness
// functions are called by the probe, and the thread is in JNI/application/native state. Thus we need call
// into VM to switch the thread state and VM will then call into mmtk-core again to do the actual work of
// harness_begin() and harness_end()

#[no_mangle]
pub extern "C" fn harness_begin(_id: usize) {
    unsafe { ((*UPCALLS).harness_begin)() };
}

#[no_mangle]
pub extern "C" fn mmtk_harness_begin_impl() {
    // Pass null as tls, OpenJDK binding does not rely on the tls value to block the current thread and do a GC
    memory_manager::harness_begin(&SINGLETON, VMMutatorThread(VMThread::UNINITIALIZED));
}

#[no_mangle]
pub extern "C" fn harness_end(_id: usize) {
    unsafe { ((*UPCALLS).harness_end)() };
}

#[no_mangle]
pub extern "C" fn mmtk_harness_end_impl() {
    memory_manager::harness_end(&SINGLETON);
}

#[no_mangle]
pub extern "C" fn mmtk_critical_section_start(jni_env: *const libc::c_void) {
    unsafe { ((*UPCALLS).critical_section_start)(jni_env) };
}

#[no_mangle]
pub extern "C" fn mmtk_critical_section_finish(jni_env: *const libc::c_void) {
    unsafe { ((*UPCALLS).critical_section_finish)(jni_env) };
}

#[no_mangle]
// We trust the name/value pointer is valid.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn process(name: *const c_char, value: *const c_char) -> bool {
    let name_str: &CStr = unsafe { CStr::from_ptr(name) };
    let value_str: &CStr = unsafe { CStr::from_ptr(value) };
    let mut builder = BUILDER.lock().unwrap();
    memory_manager::process(
        &mut builder,
        name_str.to_str().unwrap(),
        value_str.to_str().unwrap(),
    )
}

#[no_mangle]
// We trust the name/value pointer is valid.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn process_bulk(options: *const c_char) -> bool {
    let options_str: &CStr = unsafe { CStr::from_ptr(options) };
    let mut builder = BUILDER.lock().unwrap();
    memory_manager::process_bulk(&mut builder, options_str.to_str().unwrap())
}

#[no_mangle]
pub extern "C" fn starting_heap_address() -> Address {
    memory_manager::starting_heap_address()
}

#[no_mangle]
pub extern "C" fn last_heap_address() -> Address {
    memory_manager::last_heap_address()
}

#[no_mangle]
pub extern "C" fn openjdk_max_capacity() -> usize {
    memory_manager::total_bytes(&SINGLETON)
}

#[no_mangle]
pub extern "C" fn executable() -> bool {
    true
}

/// Full pre barrier
#[no_mangle]
pub extern "C" fn mmtk_object_reference_write_pre(
    mutator: &'static mut Mutator<OpenJDK>,
    src: ObjectReference,
    slot: Address,
    target: ObjectReference,
) {
    mutator
        .barrier()
        .object_reference_write_pre(src, slot, target);
}

/// Full post barrier
#[no_mangle]
pub extern "C" fn mmtk_object_reference_write_post(
    mutator: &'static mut Mutator<OpenJDK>,
    src: ObjectReference,
    slot: Address,
    target: ObjectReference,
) {
    mutator
        .barrier()
        .object_reference_write_post(src, slot, target);
}

/// Barrier slow-path call
#[no_mangle]
pub extern "C" fn mmtk_object_reference_write_slow(
    mutator: &'static mut Mutator<OpenJDK>,
    src: ObjectReference,
    slot: Address,
    target: ObjectReference,
) {
    mutator
        .barrier()
        .object_reference_write_slow(src, slot, target);
}

/// Barrier slow-path call
#[no_mangle]
pub extern "C" fn mmtk_object_reference_read_slow(
    mutator: &'static mut Mutator<OpenJDK>,
    target: ObjectReference,
) {
    mutator.barrier().object_reference_read_slow(target);
}

/// Array-copy pre-barrier
#[no_mangle]
pub extern "C" fn mmtk_array_copy_pre(
    mutator: &'static mut Mutator<OpenJDK>,
    src: Address,
    dst: Address,
    count: usize,
) {
    let bytes = count << LOG_BYTES_IN_ADDRESS;
    mutator
        .barrier()
        .memory_region_copy_pre(src..src + bytes, dst..dst + bytes);
}

/// Array-copy post-barrier
#[no_mangle]
pub extern "C" fn mmtk_array_copy_post(
    mutator: &'static mut Mutator<OpenJDK>,
    src: Address,
    dst: Address,
    count: usize,
) {
    let bytes = count << LOG_BYTES_IN_ADDRESS;
    mutator
        .barrier()
        .memory_region_copy_post(src..src + bytes, dst..dst + bytes);
}

// finalization
#[no_mangle]
pub extern "C" fn add_finalizer(object: ObjectReference) {
    memory_manager::add_finalizer(&SINGLETON, object);
}

#[no_mangle]
pub extern "C" fn get_finalized_object() -> ObjectReference {
    match memory_manager::get_finalized_object(&SINGLETON) {
        Some(obj) => obj,
        None => unsafe { Address::ZERO.to_object_reference() },
    }
}

thread_local! {
    /// Cache all the pointers reported by the current thread.
    static NMETHOD_SLOTS: RefCell<Vec<Address>> = RefCell::new(vec![]);
}

/// Report a list of pointers in nmethod to mmtk.
#[no_mangle]
pub extern "C" fn mmtk_add_nmethod_oop(addr: Address) {
    NMETHOD_SLOTS.with(|x| x.borrow_mut().push(addr))
}

/// Register a nmethod.
/// The c++ part of the binding should scan the nmethod and report all the pointers to mmtk first, before calling this function.
/// This function will transfer all the locally cached pointers of this nmethod to the global storage.
#[no_mangle]
pub extern "C" fn mmtk_register_nmethod(nm: Address) {
    let slots = NMETHOD_SLOTS.with(|x| {
        if x.borrow().len() == 0 {
            return None;
        }
        Some(x.replace(vec![]))
    });
    let slots = match slots {
        Some(slots) => slots,
        _ => return,
    };
    let mut roots = crate::CODE_CACHE_ROOTS.lock().unwrap();
    // Relaxed add instead of `fetch_add`, since we've already acquired the lock.
    crate::CODE_CACHE_ROOTS_SIZE.store(
        crate::CODE_CACHE_ROOTS_SIZE.load(Ordering::Relaxed) + slots.len(),
        Ordering::Relaxed,
    );
    roots.insert(nm, slots);
}

/// Unregister a nmethod.
#[no_mangle]
pub extern "C" fn mmtk_unregister_nmethod(nm: Address) {
    let mut roots = crate::CODE_CACHE_ROOTS.lock().unwrap();
    if let Some(slots) = roots.remove(&nm) {
        // Relaxed sub instead of `fetch_sub`, since we've already acquired the lock.
        crate::CODE_CACHE_ROOTS_SIZE.store(
            crate::CODE_CACHE_ROOTS_SIZE.load(Ordering::Relaxed) - slots.len(),
            Ordering::Relaxed,
        );
    }
}

#[no_mangle]
pub extern "C" fn mmtk_threadlocal_closure(
    tls: VMMutatorThread,
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    visited: *mut std::collections::HashSet<ObjectReference>,
) -> NewBuffer {
    use crate::mmtk::vm::VMBinding;
    use mmtk::vm::ActivePlan;
    const CAPACITY: usize = 4096;
    if !ptr.is_null() {
        assert!(!visited.is_null(), "visited list is null");
        let buf = unsafe { Vec::<Address>::from_raw_parts(ptr, length, capacity) };
        let m = <OpenJDK as VMBinding>::VMActivePlan::mutator(tls);
        let set = unsafe {
            assert!(!visited.is_null());
            &mut *visited
        };
        memory_manager::mmtk_threadlocal_closure(&SINGLETON, m, buf, set);
    }
    let (ptr, _, capacity) = {
        // TODO: Use Vec::into_raw_parts() when the method is available.
        use std::mem::ManuallyDrop;
        let new_vec = Vec::with_capacity(CAPACITY);
        let mut me = ManuallyDrop::new(new_vec);
        (me.as_mut_ptr(), me.len(), me.capacity())
    };

    NewBuffer { ptr, capacity }
}

#[no_mangle]
pub extern "C" fn mmtk_post_threadlocal_closure(tls: VMMutatorThread) {
    use crate::mmtk::vm::ActivePlan;
    use crate::mmtk::vm::VMBinding;

    let mut mutator = <OpenJDK as VMBinding>::VMActivePlan::mutator(tls);

    let s = mutator.barrier.statistics();
    mutator.critical_section_write_barrier_public_counter = s.0;
    mutator.critical_section_write_barrier_public_bytes = s.1;
    mutator.critical_section_write_barrier_counter = s.2;
    mutator.critical_section_write_barrier_slowpath_counter = s.3;
    // mutator.access_non_local_object_counter = s.4 as u32;
    // mutator.set_public_counter += s.2;
}

#[no_mangle]
pub extern "C" fn mmtk_reset_barier_statistics(tls: VMMutatorThread, mutator_id: usize) {
    use crate::mmtk::vm::ActivePlan;
    use crate::mmtk::vm::VMBinding;

    let mut mutator = <OpenJDK as VMBinding>::VMActivePlan::mutator(tls);
    mutator.barrier.reset_statistics(mutator_id);
}

#[no_mangle]
pub unsafe extern "C" fn release_visited_buffer(
    ptr: *mut std::collections::HashSet<ObjectReference>,
) {
    Box::from_raw(ptr);
}

#[no_mangle]
pub unsafe extern "C" fn mmtk_new_visited_set() -> *mut std::collections::HashSet<ObjectReference> {
    Box::into_raw(Box::new(std::collections::HashSet::new()))
}

#[no_mangle]
pub extern "C" fn mmtk_set_public_bit(tls: VMMutatorThread, objecct: ObjectReference, force: bool) {
    use crate::mmtk::vm::ActivePlan;
    use crate::mmtk::vm::VMBinding;
    let mutator_id = <OpenJDK as VMBinding>::VMActivePlan::mutator_id(tls);
    let mutator = <OpenJDK as VMBinding>::VMActivePlan::mutator(tls);
    memory_manager::mmtk_set_public_bit(
        objecct,
        mutator_id,
        (mutator.request_id as usize) << 32 | mutator_id,
        force,
    );
}

#[no_mangle]
pub unsafe extern "C" fn mmtk_write_log_file(log: *const libc::c_char, tls: VMMutatorThread) {
    // use crate::mmtk::vm::ActivePlan;
    // use crate::mmtk::vm::VMBinding;
    // use std::io::prelude::*;

    // let mutator = <OpenJDK as VMBinding>::VMActivePlan::mutator(tls);
    // let file_name = CStr::from_ptr(log).to_str().unwrap();
    // // println!("{}", file_name);
    // let mut log_file = std::fs::OpenOptions::new()
    //     .write(true)
    //     .append(true)
    //     .create(true)
    //     .open(file_name)
    //     .unwrap();
    // writeln!(
    //     log_file,
    //     "{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}",
    //     mutator.critical_section_total_object_counter,
    //     mutator.critical_section_total_object_bytes,
    //     mutator.critical_section_total_local_object_counter,
    //     mutator.critical_section_total_local_object_bytes,
    //     mutator.critical_section_local_live_object_counter,
    //     mutator.critical_section_local_live_object_bytes,
    //     mutator.critical_section_local_live_private_object_counter,
    //     mutator.critical_section_local_live_private_object_bytes,
    //     mutator.critical_section_write_barrier_counter,
    //     mutator.critical_section_write_barrier_slowpath_counter,
    //     mutator.critical_section_write_barrier_public_counter,
    //     mutator.critical_section_write_barrier_public_bytes,
    //     mutator.set_public_counter,
    //     mutator.access_non_local_object_counter,
    // )
    // .unwrap();
}
