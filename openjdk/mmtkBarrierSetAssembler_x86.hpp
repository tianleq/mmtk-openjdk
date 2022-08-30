#ifndef MMTK_OPENJDK_MMTK_BARRIER_SET_ASSEMBLER_X86_HPP
#define MMTK_OPENJDK_MMTK_BARRIER_SET_ASSEMBLER_X86_HPP

#include "asm/macroAssembler.hpp"
#include "gc/shared/barrierSetAssembler.hpp"

class MMTkBarrierSetAssembler: public BarrierSetAssembler {
public:
  virtual void eden_allocate(MacroAssembler* masm, Register thread, Register obj, Register var_size_in_bytes, int con_size_in_bytes, Register t1, Label& slow_case);
  virtual void store_at(MacroAssembler* masm, DecoratorSet decorators, BasicType type, Address dst, Register val, Register tmp1, Register tmp2) {
    BarrierSetAssembler::store_at(masm, decorators, type, dst, val, tmp1, tmp2);
  }
  virtual void load_at(MacroAssembler* masm, DecoratorSet decorators, BasicType type,
                       Register dst, Address src, Register tmp1, Register tmp_thread) {
    BarrierSetAssembler::load_at(masm, decorators, type, dst, src, tmp1, tmp_thread);
  }
};
#endif // MMTK_OPENJDK_MMTK_BARRIER_SET_ASSEMBLER_X86_HPP
