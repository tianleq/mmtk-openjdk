#include "precompiled.hpp"
#include "mmtkPublicObjectMarkingBarrier.hpp"
#include "runtime/interfaceSupport.inline.hpp"
#include "c1/c1_LIRAssembler.hpp"

// #define MMTK_ENABLE_BARRIER_FASTPATH true

void MMTkPublicObjectMarkingBarrierSetRuntime::object_reference_write_mid_call(void* src, void* slot, void* target) {
  // if target is null, nothing needs to be published
  if (!target) return;
  // Now check target/value, only go to slow-path when target is private/has not been published
  intptr_t addr = (intptr_t) (void*) target;
  uint8_t* meta_addr = (uint8_t*) (PUBLIC_BIT_BASE_ADDRESS + (addr >> 6));
  intptr_t shift = (addr >> 3) & 0b111;
  uint8_t byte_val = *meta_addr;
  if (((byte_val >> shift) & 1) != 1) {
    MMTkBarrierSetRuntime::object_reference_write_slow_call(src, slot, target);
  }
}

void MMTkPublicObjectMarkingBarrierSetRuntime::object_reference_write_pre(oop src, oop* slot, oop target) const {
  if (mmtk_enable_barrier_fastpath) {
    intptr_t addr = (intptr_t) (void*) src;
    uint8_t* meta_addr = (uint8_t*) (PUBLIC_BIT_BASE_ADDRESS + (addr >> 6));
    intptr_t shift = (addr >> 3) & 0b111;
    uint8_t byte_val = *meta_addr;
    if (((byte_val >> shift) & 1) == 1) {
      MMTkPublicObjectMarkingBarrierSetRuntime::object_reference_write_mid_call((void*) src, (void*) slot, (void*) target);
    }
  } else {
    object_reference_write_pre_call((void*) src, (void*) slot, (void*) target);
  }
}

#define __ masm->

void MMTkPublicObjectMarkingBarrierSetAssembler::object_reference_write_pre(MacroAssembler* masm, DecoratorSet decorators, Address dst, Register val, Register tmp1, Register tmp2) const {
  if (can_remove_barrier(decorators, val, /* skip_const_null */ true)) return;
  Register obj = dst.base();
  if (mmtk_enable_barrier_fastpath) {
    Label done;

    Register tmp3 = rscratch1;
    Register tmp4 = rscratch2;
    assert_different_registers(obj, tmp2, tmp3);
    assert_different_registers(tmp4, rcx);
    __ pusha();
    // tmp2 = load-byte (PUBLIC_BIT_BASE_ADDRESS + (obj >> 6));
    __ movptr(tmp3, obj);
    __ shrptr(tmp3, 6);
    __ movptr(tmp2, PUBLIC_BIT_BASE_ADDRESS);
    __ movb(tmp2, Address(tmp2, tmp3));
    // tmp3 = (obj >> 3) & 7
    __ movptr(tmp3, obj);
    __ shrptr(tmp3, 3);
    __ andptr(tmp3, 7);
    // tmp2 = tmp2 >> tmp3
    __ movptr(tmp4, rcx);
    __ movl(rcx, tmp3);
    __ shrptr(tmp2);
    __ movptr(rcx, tmp4);
    // if ((tmp2 & 1) == 1) goto slowpath; only go to slow path when src object is public
    __ andptr(tmp2, 1);
    __ cmpptr(tmp2, 1);
    __ jcc(Assembler::notEqual, done);

    __ movptr(c_rarg0, obj);
    __ lea(c_rarg1, dst);
    __ movptr(c_rarg2, val == noreg ?  (int32_t) NULL_WORD : val);
    __ call_VM_leaf_base(FN_ADDR(MMTkPublicObjectMarkingBarrierSetRuntime::object_reference_write_mid_call), 3);
    __ bind(done);
    __ popa();
  } else {
    __ pusha();
    __ movptr(c_rarg0, obj);
    __ lea(c_rarg1, dst);
    // __ movptr(c_rarg2, val == noreg ?  (int32_t) NULL_WORD : val);
    __ movptr(c_rarg2, val);
    __ call_VM_leaf_base(FN_ADDR(MMTkBarrierSetRuntime::object_reference_write_pre_call), 3);
    __ popa();
  }

}


void MMTkPublicObjectMarkingBarrierSetAssembler::oop_arraycopy_prologue(MacroAssembler* masm, DecoratorSet decorators, BasicType type, 
                                                                        Register src_oop, Register dst_oop, Register src, Register dst, Register count) {
  // `count` or `dst` register values may get overwritten after the array copy, and `arraycopy_epilogue` can receive invalid addresses.
  // Save the register values here and restore them in `arraycopy_epilogue`.
  // See https://github.com/openjdk/jdk/blob/jdk-11%2B19/src/hotspot/cpu/x86/gc/shared/modRefBarrierSetAssembler_x86.cpp#L37-L50

  bool checkcast = (decorators & ARRAYCOPY_CHECKCAST) != 0;
  bool disjoint = (decorators & ARRAYCOPY_DISJOINT) != 0;
  bool obj_int = type == T_OBJECT LP64_ONLY(&& UseCompressedOops);

  if (type == T_OBJECT || type == T_ARRAY) {

  assert(src_oop == rscratch1, "src oop is not in r10");
  assert(dst_oop == rscratch2, "dst oop is not in r11");
  assert(src == c_rarg0, "src oop is not in rdi");
  assert(dst == c_rarg1, "dst oop is not in rsi");
  assert(count == c_rarg2, "dst oop is not in rdx");
  // No epilogue is needed, so skip the save

// #ifdef _LP64
//     if (!checkcast) {
//       if (!obj_int) {
//         // Save count for barrier
//         __ movptr(r11, count);
//       } else if (disjoint) {
//         // Save dst in r11 in the disjoint case
//         __ movq(r11, dst);
//       }
//     }
// #else
//     if (disjoint) {
//       __ mov(rdx, dst);          // save 'to'
//     }
// #endif
    // saving all regs is necessary as this prologue does not have its own frame
    // it has to make sure no registers get killed
    __ pusha();
    __ movptr(c_rarg0, src);
    __ movptr(c_rarg1, dst);
    __ movptr(c_rarg2, count);
    __ movptr(c_rarg3, src_oop);
    __ movptr(c_rarg4, dst_oop);
    __ call_VM_leaf_base(FN_ADDR(MMTkBarrierSetRuntime::object_reference_array_copy_pre_call), 5);
    __ popa();
  }
}

#undef __

#define __ sasm->
// void MMTkPublicObjectMarkingBarrierSetAssembler::generate_c1_write_barrier_runtime_stub(StubAssembler* sasm) const {
//   __ prologue("mmtk_write_barrier", false);

//   Address store_addr(rbp, 4*BytesPerWord);

//   Label done, runtime;

//   __ push(c_rarg0);
//   __ push(c_rarg1);
//   __ push(c_rarg2);
//   __ push(rax);

//   __ load_parameter(0, c_rarg0);
//   __ load_parameter(1, c_rarg1);
//   __ load_parameter(2, c_rarg2);

//   __ bind(runtime);

//   __ save_live_registers_no_oop_map(true);

// #if MMTK_ENABLE_BARRIER_FASTPATH
//   __ call_VM_leaf_base(FN_ADDR(MMTkPublicObjectMarkingBarrierSetRuntime::object_reference_write_mid_call), 3);
// #else
//   __ call_VM_leaf_base(FN_ADDR(MMTkBarrierSetRuntime::object_reference_write_pre_call), 3);
// #endif

//   __ restore_live_registers(true);

//   __ bind(done);
//   __ pop(rax);
//   __ pop(c_rarg2);
//   __ pop(c_rarg1);
//   __ pop(c_rarg0);

//   __ epilogue();
// }


static void __generate_c1_runtime_stub(StubAssembler* sasm, const char* name, address entry_point, int argc) {
  __ prologue(name, false);
  __ save_live_registers_no_oop_map(true);

  Address store_addr(rbp, 4*BytesPerWord);

  Label done, runtime;

  __ push(c_rarg0);
  __ push(c_rarg1);
  __ push(c_rarg2);
  __ push(rax);

  __ load_parameter(0, c_rarg0);
  __ load_parameter(1, c_rarg1);
  __ load_parameter(2, c_rarg2);

  __ bind(runtime);

  __ save_live_registers_no_oop_map(true);

  if (mmtk_enable_barrier_fastpath) {
    __ call_VM_leaf_base(FN_ADDR(MMTkPublicObjectMarkingBarrierSetRuntime::object_reference_write_mid_call), 3);
  } else {
    __ call_VM_leaf_base(FN_ADDR(MMTkBarrierSetRuntime::object_reference_write_pre_call), 3);
  }
  __ restore_live_registers(true);

  __ bind(done);
  __ pop(rax);
  __ pop(c_rarg2);
  __ pop(c_rarg1);
  __ pop(c_rarg0);

  __ epilogue();
}


void MMTkPublicObjectMarkingBarrierSetAssembler::generate_c1_object_reference_write_pre_runtime_stub(StubAssembler* sasm) {
  __generate_c1_runtime_stub(sasm, "c1_object_reference_write_pre_stub", FN_ADDR(MMTkBarrierSetRuntime::object_reference_write_pre_call), 3);
}

void MMTkPublicObjectMarkingBarrierSetAssembler::generate_c1_object_reference_write_pre_imprecise_runtime_stub(StubAssembler* sasm) {
  __generate_c1_runtime_stub(sasm, "c1_object_reference_write_preimprecise_stub", FN_ADDR(MMTkBarrierSetRuntime::object_reference_write_pre_call), 3);
}

#undef __

#define __ ce->masm()->

void MMTkPublicObjectMarkingBarrierSetAssembler::generate_c1_pre_write_barrier_stub(LIR_Assembler* ce, MMTkC1PreBarrierStub* stub) const {
  MMTkBarrierSetC1* bs = (MMTkBarrierSetC1*) BarrierSet::barrier_set()->barrier_set_c1();
  __ bind(*stub->entry());
  address runtime_address;
  if (stub->patch_code != lir_patch_none) {
    // In this case, the value of slot is incorrect, the following code simply makes LIRAssembler happy
    // When code needs patching, all fields of the src object need to be remembered
    
    // Patch
    assert(stub->scratch->is_single_cpu(), "must be");
    assert(stub->scratch->is_register(), "Precondition.");
    ce->mem2reg(stub->slot, stub->scratch, T_OBJECT, stub->patch_code, stub->info, false /*wide*/, false /*unaligned*/);
    // Resolve address
    auto masm = ce->masm();
    LIR_Address* addr = stub->slot->as_address_ptr();
    Address from_addr = ce->as_Address_ext(addr);
    __ lea(stub->scratch->as_register(), from_addr);
    // Store parameter
    ce->store_parameter(stub->scratch->as_pointer_register(), 1);
    runtime_address = bs->object_reference_write_pre_imprecise_c1_runtime_code_blob()->code_begin();
  } else {
    // Store parameter
    ce->store_parameter(stub->slot->as_pointer_register(), 1);
    runtime_address = bs->object_reference_write_pre_c1_runtime_code_blob()->code_begin();
  }

  // Store parameter
  ce->store_parameter(stub->src->as_pointer_register(), 0);
  ce->store_parameter(stub->new_val->as_pointer_register(), 2);

  __ call(RuntimeAddress(runtime_address));
  __ jmp(*stub->continuation());
}

#undef __


#ifdef ASSERT
#define __ gen->lir(__FILE__, __LINE__)->
#else
#define __ gen->lir()->
#endif


void MMTkPublicObjectMarkingBarrierSetC1::generate_c1_runtime_stubs(BufferBlob* buffer_blob) {
  using GenStubFunc = void(*)(StubAssembler*);
  class RuntimeCodeBlobCodeGenClosure : public StubAssemblerCodeGenClosure {
    GenStubFunc gen_stub;
  public:
    RuntimeCodeBlobCodeGenClosure(GenStubFunc gen_stub): gen_stub(gen_stub) {}

    virtual OopMapSet* generate_code(StubAssembler* sasm) override {
      gen_stub(sasm);
      return nullptr;
    }
  };

  auto do_code_blob = [buffer_blob](const char* name, GenStubFunc gen_stub) {
    RuntimeCodeBlobCodeGenClosure closure(gen_stub);
    return Runtime1::generate_blob(buffer_blob, -1, name, false, &closure);
  };

  _load_reference_c1_runtime_code_blob              = do_code_blob("c1_load_reference_stub",              &MMTkBarrierSetAssembler::generate_c1_load_reference_runtime_stub);
  _object_reference_write_pre_c1_runtime_code_blob  = do_code_blob("c1_object_reference_write_pre_stub",  &MMTkPublicObjectMarkingBarrierSetAssembler::generate_c1_object_reference_write_pre_runtime_stub);
  _object_reference_write_post_c1_runtime_code_blob = do_code_blob("c1_object_reference_write_post_stub", &MMTkBarrierSetAssembler::generate_c1_object_reference_write_post_runtime_stub);
  _object_reference_write_slow_c1_runtime_code_blob = do_code_blob("c1_object_reference_write_slow_stub", &MMTkBarrierSetAssembler::generate_c1_object_reference_write_slow_runtime_stub);
  _object_reference_write_pre_imprecise_c1_runtime_code_blob = do_code_blob("c1_object_reference_write_pre_imprecise_stub",  &MMTkPublicObjectMarkingBarrierSetAssembler::generate_c1_object_reference_write_pre_imprecise_runtime_stub);
}

void MMTkPublicObjectMarkingBarrierSetC1::object_reference_write_pre(LIRAccess& access, LIR_Opr src, LIR_Opr slot, LIR_Opr new_val) const {
  LIRGenerator* gen = access.gen();
  DecoratorSet decorators = access.decorators();
  if ((decorators & IN_HEAP) == 0) return;
  if (!src->is_register()) {
    LIR_Opr reg = gen->new_pointer_register();
    if (src->is_constant()) {
      __ move(src, reg);
    } else {
      __ leal(src, reg);
    }
    src = reg;
  }
  assert(src->is_register(), "must be a register at this point");
  if (!slot->is_register()) {
    LIR_Opr reg = gen->new_pointer_register();
    if (slot->is_constant()) {
      __ move(slot, reg);
    } else {
      __ leal(slot, reg);
    }
    slot = reg;
  }
  assert(slot->is_register(), "must be a register at this point");
  if (!new_val->is_register()) {
    LIR_Opr new_val_reg = gen->new_register(T_OBJECT);
    if (new_val->is_constant()) {
      __ move(new_val, new_val_reg);
    } else {
      __ leal(new_val, new_val_reg);
    }
    new_val = new_val_reg;
  }
  assert(new_val->is_register(), "must be a register at this point");
  CodeStub* slow = new MMTkC1PreBarrierStub(src, slot, new_val);

  if (mmtk_enable_barrier_fastpath) {
    LIR_Opr addr = src;
    // uint8_t* meta_addr = (uint8_t*) (PUBLIC_BIT_BASE_ADDRESS + (addr >> 6));
    LIR_Opr offset = gen->new_pointer_register();
    __ move(addr, offset);
    __ unsigned_shift_right(offset, 6, offset);
    LIR_Opr base = gen->new_pointer_register();
    __ move(LIR_OprFact::longConst(PUBLIC_BIT_BASE_ADDRESS), base);
    LIR_Address* meta_addr = new LIR_Address(base, offset, T_BYTE);
    // uint8_t byte_val = *meta_addr;
    LIR_Opr byte_val = gen->new_register(T_INT);
    __ move(meta_addr, byte_val);
    // intptr_t shift = (addr >> 3) & 0b111;
    LIR_Opr shift = gen->new_register(T_INT);
    __ move(addr, shift);
    __ unsigned_shift_right(shift, 3, shift);
    __ logical_and(shift, LIR_OprFact::intConst(0b111), shift);
    // if (((byte_val >> shift) & 1) == 1) slow;
    LIR_Opr result = byte_val;
    __ unsigned_shift_right(result, shift, result, LIR_OprFact::illegalOpr);
    __ logical_and(result, LIR_OprFact::intConst(1), result);
    __ cmp(lir_cond_equal, result, LIR_OprFact::intConst(1));
    __ branch(lir_cond_equal, T_BYTE, slow);
  } else {
    __ jump(slow);
  }

  __ branch_destination(slow->continuation());
}

#undef __

#define __ ideal.

// The only case that the barrier can be skiped is that the target is null
bool MMTkPublicObjectMarkingBarrierSetC2::can_remove_barrier(GraphKit* kit, PhaseTransform* phase, Node* src, Node* slot, Node* val, bool skip_const_null) const {
  // Skip barrier if the new target is a null pointer.
  if (skip_const_null && val != NULL && val->is_Con() && val->bottom_type() == TypePtr::NULL_PTR) {
    return true;
  }
  return false;
}

void MMTkPublicObjectMarkingBarrierSetC2::object_reference_write_pre(GraphKit* kit, Node* src, Node* slot, Node* val) const {
  if (can_remove_barrier(kit, &kit->gvn(), src, slot, val, /* skip_const_null */ true)) return;

  MMTkIdealKit ideal(kit, true);

  if (mmtk_enable_barrier_fastpath) {
    Node* no_base = __ top();
    float unlikely  = PROB_UNLIKELY(0.95);

    Node* zero  = __ ConI(0);
    Node* addr = __ CastPX(__ ctrl(), src);
    Node* meta_addr = __ AddP(no_base, __ ConP(PUBLIC_BIT_BASE_ADDRESS), __ URShiftX(addr, __ ConI(6)));
    Node* byte = __ load(__ ctrl(), meta_addr, TypeInt::INT, T_BYTE, Compile::AliasIdxRaw);
    Node* shift = __ URShiftX(addr, __ ConI(3));
    shift = __ AndI(__ ConvL2I(shift), __ ConI(7));
    Node* result = __ AndI(__ URShiftI(byte, shift), __ ConI(1));

    __ if_then(result, BoolTest::ne, zero, unlikely); {
      const TypeFunc* tf = __ func_type(TypeOopPtr::BOTTOM, TypeOopPtr::BOTTOM, TypeOopPtr::BOTTOM);
      Node* x = __ make_leaf_call(tf, FN_ADDR(MMTkPublicObjectMarkingBarrierSetRuntime::object_reference_write_mid_call), "mmtk_barrier_call", src, slot, val);
    } __ end_if();
  } else {
    const TypeFunc* tf = __ func_type(TypeOopPtr::BOTTOM, TypeOopPtr::BOTTOM, TypeOopPtr::BOTTOM);
    Node* x = __ make_leaf_call(tf, FN_ADDR(MMTkBarrierSetRuntime::object_reference_write_pre_call), "mmtk_barrier_call", src, slot, val);
  }

  kit->final_sync(ideal); // Final sync IdealKit and GraphKit.
}

#undef __
