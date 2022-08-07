use crate::NewBuffer;
use crate::OpenJDK;
use crate::OpenJDK_Upcalls;
use crate::SINGLETON;
use crate::UPCALLS;
use libc::c_char;
use mmtk::memory_manager;
use mmtk::plan::BarrierSelector;
use mmtk::scheduler::GCController;
use mmtk::scheduler::GCWorker;
use mmtk::util::alloc::AllocatorSelector;
use mmtk::util::opaque_pointer::*;
use mmtk::util::{Address, ObjectReference};
use mmtk::AllocationSemantics;
use mmtk::Mutator;
use mmtk::MutatorContext;
use mmtk::MMTK;
use once_cell::sync;
use std::ffi::{CStr, CString};

// Supported barriers:
static NO_BARRIER: sync::Lazy<CString> = sync::Lazy::new(|| CString::new("NoBarrier").unwrap());
static OBJECT_BARRIER: sync::Lazy<CString> =
    sync::Lazy::new(|| CString::new("ObjectBarrier").unwrap());

static OBJECT_OWNER_BARRIER: sync::Lazy<CString> =
    sync::Lazy::new(|| CString::new("ObjectOwnerBarrier").unwrap());

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
pub extern "C" fn openjdk_gc_init(calls: *const OpenJDK_Upcalls, heap_size: usize) {
    unsafe { UPCALLS = calls };
    crate::abi::validate_memory_layouts();
    // MMTk should not be used before gc_init, and gc_init is single threaded. It is fine we get a mutable reference from the singleton.
    #[allow(clippy::cast_ref_to_mut)]
    let singleton_mut =
        unsafe { &mut *(&*SINGLETON as *const MMTK<OpenJDK> as *mut MMTK<OpenJDK>) };
    memory_manager::gc_init(singleton_mut, heap_size);
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
    // println!("start critical section");
    unsafe { ((*UPCALLS).critical_section_start)(jni_env) };
    // memory_manager::critical_section_start(&SINGLETON);
    // SINGLETON
    //     .get_plan()
    //     .base()
    //     .stress_enabled
    //     .store(true, std::sync::atomic::Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn mmtk_critical_section_finish(jni_env: *const libc::c_void) {
    // memory_manager::critical_section_finish(&SINGLETON);
    unsafe { ((*UPCALLS).critical_section_finish)(jni_env) };
}

#[no_mangle]
pub extern "C" fn mmtk_enable_stress(jni_env: *const libc::c_void) {
    SINGLETON
        .get_plan()
        .base()
        .stress_enabled
        .store(true, std::sync::atomic::Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn mmtk_global_gc_id() -> usize {
    SINGLETON
        .get_plan()
        .base()
        .gc_id
        .load(std::sync::atomic::Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn mmtk_do_explicit_gc(tls: VMMutatorThread) {
    // use crate::mmtk::vm::VMBinding;
    // use mmtk::vm::ActivePlan;

    // SINGLETON
    //     .get_plan()
    //     .base()
    //     .mutators
    //     .lock()
    //     .unwrap()
    //     .push(tls);
    // let m = <OpenJDK as VMBinding>::VMActivePlan::mutator(tls);
    // println!(
    //     "gc: {}, push mutator: {} -- request: {}, active: {}",
    //     SINGLETON
    //         .get_plan()
    //         .base()
    //         .gc_counter
    //         .load(std::sync::atomic::Ordering::SeqCst),
    //     <OpenJDK as VMBinding>::VMActivePlan::mutator_id(tls),
    //     m.request_id,
    //     m.critical_section_active
    // );
    // assert!(!m.critical_section_active, "race/gc issue");
    SINGLETON
        .get_plan()
        .handle_user_collection_request(tls, true);
}

#[no_mangle]
pub extern "C" fn mmtk_print_thread_stack() {
    unsafe { ((*UPCALLS).print_thread_stack)() };
}

#[no_mangle]
// We trust the name/value pointer is valid.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn process(name: *const c_char, value: *const c_char) -> bool {
    let name_str: &CStr = unsafe { CStr::from_ptr(name) };
    let value_str: &CStr = unsafe { CStr::from_ptr(value) };
    memory_manager::process(
        &SINGLETON,
        name_str.to_str().unwrap(),
        value_str.to_str().unwrap(),
    )
}

#[no_mangle]
// We trust the name/value pointer is valid.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn process_bulk(options: *const c_char) -> bool {
    let options_str: &CStr = unsafe { CStr::from_ptr(options) };
    memory_manager::process_bulk(&SINGLETON, options_str.to_str().unwrap())
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

#[no_mangle]
pub extern "C" fn record_modified_node(
    mutator: &'static mut Mutator<OpenJDK>,
    obj: ObjectReference,
) {
    mutator.record_modified_node(obj);
}

#[no_mangle]
pub extern "C" fn record_non_local_object(
    mutator: &'static mut Mutator<OpenJDK>,
    obj: ObjectReference,
    new_val: ObjectReference,
) {
    mutator.record_non_local_object(obj, new_val);
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

const CAPACITY: usize = 4096;

#[no_mangle]
pub extern "C" fn mmtk_threadlocal_closure(
    tls: VMMutatorThread,
    ptr: *mut Address,
    length: usize,
    capacity: usize,
) -> NewBuffer {
    use crate::mmtk::vm::VMBinding;
    use mmtk::vm::ActivePlan;
    if !ptr.is_null() {
        let buf = unsafe { Vec::<Address>::from_raw_parts(ptr, length, capacity) };
        let m = <OpenJDK as VMBinding>::VMActivePlan::mutator(tls);
        memory_manager::mmtk_threadlocal_closure(&SINGLETON, m, buf);
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

    let mutator = <OpenJDK as VMBinding>::VMActivePlan::mutator(tls);
    memory_manager::mmtk_post_threadlocal_closure(&SINGLETON);
    let c = mutator.barrier.statistics();
    // println!("{} public objects", c);
    mutator.barrier.reset_statistics();
}
