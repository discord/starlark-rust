/*
 * Copyright 2019 The Starlark in Rust Authors.
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Instruction arguments.

use std::{
    fmt,
    fmt::{Display, Formatter, Write},
};

use gazebo::dupe::Dupe;

use crate::{
    codemap::Span,
    collections::{symbol_map::Symbol, Hashed, SmallMap},
    environment::slots::ModuleSlotId,
    eval::{
        bc::{
            addr::{BcAddr, BcAddrOffset, BcPtrAddr},
            compiler::call::ArgsCompiledValueBc,
            instr::BcInstr,
            instr_impl::InstrDefData,
            opcode::{BcOpcode, BcOpcodeHandler},
        },
        runtime::slots::LocalSlotId,
    },
    values::{typed::FrozenValueTyped, FrozenRef, FrozenStringValue, FrozenValue, StarlarkValue},
};

/// Truncate value if it is too long.
struct TruncateValueRepr(FrozenValue);

impl Display for TruncateValueRepr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let repr = self.0.to_value().to_repr();
        // Truncate too long constants (like dicts with hundreds of elements).
        if repr.len() > 100 {
            write!(f, "<{}>", self.0.to_value().get_type())
        } else {
            write!(f, "{}", repr)
        }
    }
}

/// Instruction fixed argument.
pub(crate) trait BcInstrArg {
    /// Append space then append the argument, or append nothing if the argument is empty.
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result;
    /// How many additional stack elements this instruction pops.
    fn pops_stack(param: &Self) -> u32;
    /// How many additional stack elements this instruction pushes.
    fn pushes_stack(param: &Self) -> u32;
}

impl BcInstrArg for () {
    fn fmt_append(_param: &Self, _f: &mut dyn Write) -> fmt::Result {
        Ok(())
    }

    fn pops_stack((): &()) -> u32 {
        0
    }

    fn pushes_stack((): &()) -> u32 {
        0
    }
}

impl BcInstrArg for u32 {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {}", param)
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl<A: BcInstrArg, B: BcInstrArg> BcInstrArg for (A, B) {
    fn fmt_append((a, b): &Self, f: &mut dyn Write) -> fmt::Result {
        A::fmt_append(a, f)?;
        B::fmt_append(b, f)?;
        Ok(())
    }

    fn pops_stack((a, b): &Self) -> u32 {
        A::pops_stack(a) + B::pops_stack(b)
    }

    fn pushes_stack((a, b): &Self) -> u32 {
        A::pushes_stack(a) + B::pushes_stack(b)
    }
}

impl<A: BcInstrArg, B: BcInstrArg, C: BcInstrArg> BcInstrArg for (A, B, C) {
    fn fmt_append((a, b, c): &Self, f: &mut dyn Write) -> fmt::Result {
        A::fmt_append(a, f)?;
        B::fmt_append(b, f)?;
        C::fmt_append(c, f)?;
        Ok(())
    }

    fn pops_stack((a, b, c): &Self) -> u32 {
        A::pops_stack(a) + B::pops_stack(b) + C::pops_stack(c)
    }

    fn pushes_stack((a, b, c): &Self) -> u32 {
        A::pushes_stack(a) + B::pushes_stack(b) + C::pushes_stack(c)
    }
}

#[allow(clippy::many_single_char_names)]
impl<A: BcInstrArg, B: BcInstrArg, C: BcInstrArg, D: BcInstrArg> BcInstrArg for (A, B, C, D) {
    fn fmt_append((a, b, c, d): &Self, f: &mut dyn Write) -> fmt::Result {
        A::fmt_append(a, f)?;
        B::fmt_append(b, f)?;
        C::fmt_append(c, f)?;
        D::fmt_append(d, f)?;
        Ok(())
    }

    fn pops_stack((a, b, c, d): &Self) -> u32 {
        A::pops_stack(a) + B::pops_stack(b) + C::pops_stack(c) + D::pops_stack(d)
    }

    fn pushes_stack((a, b, c, d): &Self) -> u32 {
        A::pushes_stack(a) + B::pushes_stack(b) + C::pushes_stack(c) + D::pushes_stack(d)
    }
}

impl<A: BcInstrArg, const N: usize> BcInstrArg for [A; N] {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        for a in param {
            A::fmt_append(a, f)?;
        }
        Ok(())
    }

    fn pops_stack(param: &Self) -> u32 {
        let mut i = 0;
        for a in param {
            i += A::pops_stack(a);
        }
        i
    }

    fn pushes_stack(param: &Self) -> u32 {
        let mut i = 0;
        for a in param {
            i += A::pushes_stack(a);
        }
        i
    }
}

impl BcInstrArg for BcAddrOffset {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " +{}", param.0)
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for BcAddr {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {}", param.0)
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for FrozenValue {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {}", TruncateValueRepr(*param))
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for Option<FrozenValue> {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        match param {
            None => write!(f, " ()"),
            Some(v) => write!(f, " {}", TruncateValueRepr(*v)),
        }
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for FrozenStringValue {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {}", TruncateValueRepr(param.unpack()))
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for String {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, "{:?}", param)
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl<T: Display> BcInstrArg for FrozenRef<T>
where
    FrozenRef<T>: Copy,
{
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {}", param.as_ref())
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl<T: StarlarkValue<'static>> BcInstrArg for FrozenValueTyped<'static, T> {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {}", TruncateValueRepr(param.to_frozen_value()))
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for LocalSlotId {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " l{}", param.0)
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for ModuleSlotId {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " m{}", param.0)
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for Span {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {}:{}", param.begin().get(), param.end().get())
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

/// Opcode as instruction argument.
impl BcInstrArg for BcOpcode {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {:?}", param)
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

/// Instruction argument encodes how many values are popped
/// off the stack manually in the instruction impl.
#[derive(Copy, Clone, Dupe)]
pub(crate) struct ArgPopsStack(pub(crate) u32);
/// Instruction argument encodes how many values are pushed
/// to the stack manually in the instruction impl.
#[derive(Copy, Clone, Dupe)]
pub(crate) struct ArgPushesStack(pub(crate) u32);
/// Instruction arguments encodes a value is popped
/// off the stack manually in the instruction impl.
pub(crate) struct ArgPopsStack1;
/// Instruction arguments encodes a value is popped
/// off the stack manually if contained value is true.
#[derive(Copy, Clone, Dupe)]
pub(crate) struct ArgPopsStackMaybe1(pub(crate) bool);

impl BcInstrArg for ArgPushesStack {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        BcInstrArg::fmt_append(&param.0, f)
    }

    fn pops_stack(_: &Self) -> u32 {
        0
    }

    fn pushes_stack(param: &Self) -> u32 {
        param.0
    }
}

impl BcInstrArg for ArgPopsStack {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        BcInstrArg::fmt_append(&param.0, f)
    }

    fn pops_stack(pops: &Self) -> u32 {
        pops.0
    }

    fn pushes_stack(_: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for ArgPopsStack1 {
    fn fmt_append(_param: &Self, _f: &mut dyn Write) -> fmt::Result {
        Ok(())
    }

    fn pops_stack(_param: &Self) -> u32 {
        1
    }

    fn pushes_stack(_param: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for ArgPopsStackMaybe1 {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, "{}", if param.0 { 1 } else { 0 })
    }

    fn pops_stack(param: &Self) -> u32 {
        if param.0 { 1 } else { 0 }
    }

    fn pushes_stack(_param: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for Vec<(BcAddr, Span)> {
    fn fmt_append(_param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " spans")
    }

    fn pops_stack(_param: &Self) -> u32 {
        0
    }

    fn pushes_stack(_param: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for Symbol {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {}", param.as_str())
    }

    fn pops_stack(_param: &Self) -> u32 {
        0
    }

    fn pushes_stack(_param: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for Box<[FrozenValue]> {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " [")?;
        for (i, v) in param.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", TruncateValueRepr(*v))?;
        }
        write!(f, "]")?;
        Ok(())
    }

    fn pops_stack(_param: &Self) -> u32 {
        0
    }

    fn pushes_stack(_param: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for Box<[Hashed<FrozenValue>]> {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " [")?;
        for (i, v) in param.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", TruncateValueRepr(*v.key()))?;
        }
        write!(f, "]")?;
        Ok(())
    }

    fn pops_stack(_param: &Self) -> u32 {
        0
    }

    fn pushes_stack(_param: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for SmallMap<FrozenValue, FrozenValue> {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {{")?;
        for (i, (k, v)) in param.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", TruncateValueRepr(*k), TruncateValueRepr(*v))?;
        }
        write!(f, "}}")?;
        Ok(())
    }

    fn pops_stack(_param: &Self) -> u32 {
        0
    }

    fn pushes_stack(_param: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for InstrDefData {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {:?}", param)
    }

    fn pops_stack(_param: &Self) -> u32 {
        0
    }

    fn pushes_stack(_param: &Self) -> u32 {
        0
    }
}

impl BcInstrArg for ArgsCompiledValueBc {
    fn fmt_append(param: &Self, f: &mut dyn Write) -> fmt::Result {
        write!(f, " {{{}}}", param)
    }

    fn pops_stack(param: &Self) -> u32 {
        param.pos_named + if param.args { 1 } else { 0 } + if param.kwargs { 1 } else { 0 }
    }

    fn pushes_stack(_param: &Self) -> u32 {
        0
    }
}

impl BcOpcode {
    /// Format instruction argument.
    pub(crate) fn fmt_append_arg(self, ptr: BcPtrAddr, f: &mut dyn Write) -> fmt::Result {
        struct HandlerImpl<'b, 'g> {
            ptr: BcPtrAddr<'b>,
            f: &'g mut dyn Write,
        }

        impl BcOpcodeHandler<fmt::Result> for HandlerImpl<'_, '_> {
            fn handle<I: BcInstr>(self) -> fmt::Result {
                let HandlerImpl { ptr, f } = self;
                let instr = ptr.get_instr::<I>();
                I::Arg::fmt_append(&instr.arg, f)
            }
        }

        self.dispatch(HandlerImpl { ptr, f })
    }
}
