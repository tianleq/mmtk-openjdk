#include "precompiled.hpp"
#include "gc/shared/thirdPartyHeap.hpp"
#include "mmtk.h"
#include "thirdPartyHeap.hpp"
#include "thirdPartyHeapArguments.hpp"

namespace third_party_heap {

GCArguments* new_gc_arguments() {
  return NULL;
}

void register_finalizer(void* obj, Thread *thread) {
  add_finalizer(obj, &thread->third_party_heap_mutator);
}

};

// #endif
