#ifndef MMTK_OPENJDK_MMTK_PUBLISH_OBJECT_CLOSURE_HPP
#define MMTK_OPENJDK_MMTK_PUBLISH_OBJECT_CLOSURE_HPP

#include "memory/iterator.hpp"
#include "mmtk.h"
#include "oops/oop.hpp"
#include "oops/oop.inline.hpp"
#include "utilities/globalDefinitions.hpp"

// const intptr_t PUBLIC_BIT_BASE_ADDRESS = (intptr_t) GLOBAL_PUBLIC_BIT_ADDRESS;

class MMTkPublishObjectClosure : public OopClosure {

  Stack<oop, mtGC> _marking_stack;

  // EdgesClosure _edges_closure;
  // void** _buffer;
  // size_t _cap;
  // size_t _cursor;

  template <class T>
  void do_oop_work(T* p) {
    T heap_oop = RawAccess<>::oop_load(p);
    if (!CompressedOops::is_null(heap_oop)) {
      oop obj = CompressedOops::decode_not_null(heap_oop);
      oop fwd = (oop) trace_root_object(_trace, obj);

    }

  }

  void publish_oop(oop *p) {
    intptr_t addr = (intptr_t) (void*) p;
    uint8_t* meta_addr = (uint8_t*) (GLOBAL_PUBLIC_BIT_ADDRESS + (addr >> 6));
    intptr_t shift = (addr >> 3) & 0b111;
    uint8_t byte_val = *meta_addr;
    
    if (((byte_val >> shift) & 1) != 1) {
      // p needs to be published
      // no race here since by defination, private objects are accessible by its owner thread only
      *meta_addr = (byte_val )
    }
  }


public:

  virtual void do_oop(oop* p)       { do_oop_work(p); }
  virtual void do_oop(narrowOop* p) { do_oop_work(p); }
};

class MMTkScanObjectClosure : public BasicOopIterateClosure {
  void* _trace;
  CLDToOopClosure follow_cld_closure;

  template <class T>
  void do_oop_work(T* p) {
    // oop ref = (void*) oopDesc::decode_heap_oop(oopDesc::load_heap_oop(p));
    // process_edge(_trace, (void*) p);
  }

public:
  MMTkScanObjectClosure(void* trace): _trace(trace), follow_cld_closure(this, false) {}

  virtual void do_oop(oop* p)       { do_oop_work(p); }
  virtual void do_oop(narrowOop* p) {
    // printf("narrowoop edge %p -> %d %p %p\n", (void*) p, *p, *((void**) p), (void*) oopDesc::load_decode_heap_oop(p));
    do_oop_work(p);
  }

  virtual bool do_metadata() {
    return true;
  }

  virtual void do_klass(Klass* k) {
  //  follow_cld_closure.do_cld(k->class_loader_data());
    // oop op = k->klass_holder();
    // oop new_op = (oop) trace_root_object(_trace, op);
    // guarantee(new_op == op, "trace_root_object returned a different value %p -> %p", op, new_op);
  }

  virtual void do_cld(ClassLoaderData* cld) {
    follow_cld_closure.do_cld(cld);
  }

  virtual ReferenceIterationMode reference_iteration_mode() { return DO_FIELDS; }
  virtual bool idempotent() { return true; }
};



#endif // MMTK_OPENJDK_MMTK_PUBLISH_OBJECT_CLOSURE_HPP
