
#ifndef MMTK_OPENJDK_MMTK_MUTATOR_HPP
#define MMTK_OPENJDK_MMTK_MUTATOR_HPP

#include "mmtk.h"
#include "utilities/globalDefinitions.hpp"

enum Allocator {
  AllocatorDefault = 0,
  AllocatorImmortal = 1,
  AllocatorLos = 2,
  AllocatorCode = 3,
  AllocatorReadOnly = 4,
};

struct RustDynPtr {
  void* data;
  void* vtable;
};

// These constants should match the constants defind in mmtk::util::alloc::allocators
const int MAX_BUMP_ALLOCATORS = 6;
const int MAX_LARGE_OBJECT_ALLOCATORS = 2;
const int MAX_MALLOC_ALLOCATORS = 1;
const int MAX_IMMIX_ALLOCATORS = 1;
const int MAX_FREE_LIST_ALLOCATORS = 2;
const int MAX_MARK_COMPACT_ALLOCATORS = 1;

// The following types should have the same layout as the types with the same name in MMTk core (Rust)

struct BumpAllocator {
  void* tls;
  void* cursor;
  void* limit;
  RustDynPtr space;
  void* context;
};

struct LargeObjectAllocator {
  void* tls;
  void* space;
  void* context;
  void* local_los_objects;
};

struct ImmixAllocator {
  void* tls;
#ifdef MMTK_ENABLE_THREAD_LOCAL_GC
  uint32_t mutator_id;
#endif
  void* cursor;
  void* limit;
  void* immix_space;
  void* context;
  uint8_t hot;
  uint8_t copy;
  void* large_cursor;
  void* large_limit;
  uint8_t request_for_large;
  uint8_t _align[7];
  uint8_t line_opt_tag;
  uintptr_t line_opt;
#ifdef MMTK_ENABLE_THREAD_LOCAL_GC
  uint8_t sparse_line_opt_tag;
  uintptr_t sparse_line_opt;
  uintptr_t local_blocks;
  uintptr_t local_free_blocks;
  uintptr_t local_reusable_blocks;
  uint32_t semantic;
#endif
};

struct FLBlock {
  void* Address;
};

struct FLBlockList {
  FLBlock first;
  FLBlock last;
  size_t size;
  char lock;
};

struct FreeListAllocator {
  void* tls;
  void* space;
  void* context;
  FLBlockList* available_blocks;
  FLBlockList* available_blocks_stress;
  FLBlockList* unswept_blocks;
  FLBlockList* consumed_blocks;
};

struct MallocAllocator {
  void* tls;
  void* space;
  void* context;
};

struct MarkCompactAllocator {
  struct BumpAllocator bump_allocator;
};

struct Allocators {
  BumpAllocator bump_pointer[MAX_BUMP_ALLOCATORS];
  LargeObjectAllocator large_object[MAX_LARGE_OBJECT_ALLOCATORS];
  MallocAllocator malloc[MAX_MALLOC_ALLOCATORS];
  ImmixAllocator immix[MAX_IMMIX_ALLOCATORS];
  FreeListAllocator free_list[MAX_FREE_LIST_ALLOCATORS];
  MarkCompactAllocator markcompact[MAX_MARK_COMPACT_ALLOCATORS];
};

struct MutatorConfig {
  void* allocator_mapping;
  void* space_mapping;
  RustDynPtr prepare_func;
  RustDynPtr release_func;
#ifdef MMTK_ENABLE_THREAD_LOCAL_GC
  RustDynPtr thread_local_prepare_func;
  RustDynPtr thread_local_release_func;
#endif
#ifdef MMTK_ENABLE_THREAD_LOCAL_GC_COPYING
  RustDynPtr thread_local_alloc_copy_func;
  RustDynPtr thread_local_post_copy_func;
  RustDynPtr thread_local_defrag_prepare_func;
#endif
};

struct MMTkMutatorContext {
  Allocators allocators;
  RustDynPtr barrier;
  void* mutator_tls;
  RustDynPtr plan;
  MutatorConfig config;
  uint32_t mutator_id;
#ifdef MMTK_ENABLE_THREAD_LOCAL_GC
  uint32_t thread_local_gc_status;
  void* finalizable_candidates;
#endif
#if defined(MMTK_ENABLE_DEBUG_THREAD_LOCAL_GC_COPYING) || defined(MMTK_ENABLE_EXTRA_HEADER)
  size_t request_id;
#endif
#if defined(MMTK_ENABLE_EXTRA_HEADER)
  bool in_request;
  uintptr_t request_stats;
#endif
#ifdef MMTK_ENABLE_DEBUG_THREAD_LOCAL_GC_COPYING
  uintptr_t stats;
#endif
#ifdef MMTK_ENABLE_THREAD_LOCAL_GC_COPYING
  size_t local_allocation_size;
#endif

  HeapWord* alloc(size_t bytes, Allocator allocator = AllocatorDefault);

  void flush();
  void destroy();

  static MMTkMutatorContext bind(::Thread* current);
  static bool is_ready_to_bind();

  // Max object size that does not need to go into LOS. We get the value from mmtk-core, and cache its value here.
  static size_t max_non_los_default_alloc_bytes;
};
#endif // MMTK_OPENJDK_MMTK_MUTATOR_HPP
