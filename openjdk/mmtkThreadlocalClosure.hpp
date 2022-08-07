#ifndef MMTK_OPENJDK_MMTK_THREADLOCAL_CLOSURE_HPP
#define MMTK_OPENJDK_MMTK_THREADLOCAL_CLOSURE_HPP

#include "memory/iterator.hpp"
#include "mmtk.h"
#include "oops/oop.hpp"
#include "oops/oop.inline.hpp"
#include "runtime/handshake.hpp"
#include "utilities/globalDefinitions.hpp"


class MMTkThreadlocalRootsClosure : public OopClosure {

private:
  Thread* _thread;
  void** _buffer;
  size_t _cap;
  size_t _cursor;

  template <class T>
  void do_oop_work(T* p) {
    // T heap_oop = RawAccess<>::oop_load(p);
    // if (!CompressedOops::is_null(heap_oop)) {
    //   oop obj = CompressedOops::decode_not_null(heap_oop);
    //   oop fwd = (oop) trace_root_object(_trace, obj);
    //   RawAccess<>::oop_store(p, fwd);
    // }
    _buffer[_cursor++] = (void*) p;
    if (_cursor >= _cap) {
      flush();
    }
  }

  void flush() {
    if (_cursor > 0) {
      NewBuffer buf = mmtk_threadlocal_closure(_thread, _buffer, _cursor, _cap);
      _buffer = buf.buf;
      _cap = buf.cap;
      _cursor = 0;
    }
  }

public:
  MMTkThreadlocalRootsClosure(Thread *thread) : _thread(thread), _cursor(0) {
    NewBuffer buf = mmtk_threadlocal_closure(_thread, NULL, 0, 0);
    _buffer = buf.buf;
    _cap = buf.cap;
  }

  ~MMTkThreadlocalRootsClosure() {
    if (_cursor > 0) flush();
    if (_buffer != NULL) {
      release_buffer(_buffer, _cursor, _cap);
    }
  }

  virtual void do_oop(oop* p)       { do_oop_work(p); }
  virtual void do_oop(narrowOop* p) { do_oop_work(p); }
};

class MMTkThreadlocalRootsHandshakeClosure : public HandshakeClosure {
public:
  MMTkThreadlocalRootsHandshakeClosure() :
          HandshakeClosure("MMTK thread roots closure Handshake") {}

  void do_thread(Thread* thread) {
    {
      MMTkThreadlocalRootsClosure cl(thread);
      thread->oops_do(&cl, NULL);
    }
    // trace finished since the destructor of MMTkThreadlocalRootsClosure
    // is executed
    mmtk_post_threadlocal_closure(thread);
    // assert(Thread::current()->is_VM_thread(), "The threadlocal handshake should be executed by VMThread.");
  }
};

#endif