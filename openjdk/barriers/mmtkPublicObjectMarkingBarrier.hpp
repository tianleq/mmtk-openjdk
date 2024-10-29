#ifndef MMTK_OPENJDK_BARRIERS_MMTK_PUBLIC_OBJECT_MARKING_BARRIER_HPP
#define MMTK_OPENJDK_BARRIERS_MMTK_PUBLIC_OBJECT_MARKING_BARRIER_HPP

#include "../mmtk.h"
#include "../mmtkBarrierSet.hpp"
#include "../mmtkBarrierSetAssembler_x86.hpp"
#include "../mmtkBarrierSetC1.hpp"
#include "../mmtkBarrierSetC2.hpp"
#include "c1/c1_LIRAssembler.hpp"
#include "c1/c1_MacroAssembler.hpp"
#include "gc/shared/barrierSet.hpp"
#include "opto/callnode.hpp"
#include "opto/idealKit.hpp"

#ifdef MMTK_ENABLE_PUBLIC_BIT

const intptr_t PUBLIC_BIT_BASE_ADDRESS = (intptr_t) GLOBAL_PUBLIC_BIT_ADDRESS;

class MMTkPublicObjectMarkingBarrierSetRuntime: public MMTkBarrierSetRuntime {
public:
  /// Generic mid-path. Called by fast-paths.
  static void object_reference_write_mid_call(void* src, void* slot, void* target);
  // Interfaces called by `MMTkBarrierSet::AccessBarrier`
  virtual void object_reference_write_pre(oop src, oop* slot, oop target) const override;

  virtual void object_reference_array_copy_pre( oop* src, oop* dst, size_t count, oop src_base, oop dst_base) const override {
    object_reference_array_copy_pre_call((void*) src, (void*) dst, count, (void*)src_base, (void*)dst_base);
  };
};

class MMTkPublicObjectMarkingBarrierSetAssembler: public MMTkBarrierSetAssembler {
protected:
  virtual void object_reference_write_pre(MacroAssembler* masm, DecoratorSet decorators, Address dst, Register val, Register tmp1, Register tmp2) const override;
  virtual void generate_c1_write_barrier_runtime_stub(StubAssembler* sasm) const;
public:
  virtual void oop_arraycopy_prologue(MacroAssembler* masm, DecoratorSet decorators, BasicType type, 
                                      Register src_oop, Register dst_oop, Register src, Register dst, Register count) override;
  virtual bool use_oop_arraycopy_prologue() const override { 
    return true; 
  }
};

class MMTkPublicObjectMarkingBarrierSetC1: public MMTkBarrierSetC1 {
protected:
  virtual void object_reference_write_pre(LIRAccess& access, LIR_Opr src, LIR_Opr slot, LIR_Opr new_val) const override;

  virtual LIR_Opr resolve_address(LIRAccess& access, bool resolve_in_register) override {
    return MMTkBarrierSetC1::resolve_address_in_register(access, resolve_in_register);
  }
};

class MMTkPublicObjectMarkingBarrierSetC2: public MMTkBarrierSetC2 {
protected:
  virtual bool can_remove_barrier(GraphKit* kit, PhaseTransform* phase, Node* src, Node* slot, Node* val, bool skip_const_null) const;
  virtual void object_reference_write_pre(GraphKit* kit, Node* src, Node* slot, Node* val) const override;
};

// The semantic of this barrier requires a subsuming barrier 
// but c2 does not support that.
struct MMTkPublicObjectMarkingBarrier: MMTkBarrierImpl<
  MMTkPublicObjectMarkingBarrierSetRuntime,
  MMTkPublicObjectMarkingBarrierSetAssembler,
  MMTkPublicObjectMarkingBarrierSetC1,
  MMTkPublicObjectMarkingBarrierSetC2
> {};

#endif

#endif // MMTK_OPENJDK_BARRIERS_MMTK_PUBLIC_OBJECT_MARKING_BARRIER_HPP
