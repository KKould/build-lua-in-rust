use std::io::Write;
use std::rc::Rc;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use crate::bytecode::ByteCode;
use crate::value::{Value, Table};
use crate::parse::{FuncProto, MULTRET};
use crate::utils::ftoi;

// ANCHOR: print
// "print" function in Lua's std-lib.
fn lib_print(state: &mut ExeState) -> i32 {
    let mut values = Vec::with_capacity(state.get_top());
    for i in 1 ..= state.get_top() {
        values.push(state.get_value(i).to_string());
    }
    println!("{}", values.join("\t"));
    0
}
// ANCHOR_END: print

// ANCHOR: state
pub struct ExeState {
    globals: HashMap<String, Value>,
    stack: Vec::<Value>,
    base: usize, // stack base of current function
}
// ANCHOR_END: state

// ANCHOR: new
impl ExeState {
    pub fn new() -> Self {
        let mut globals = HashMap::new();
        globals.insert("print".into(), Value::RustFunction(lib_print));

        ExeState {
            globals,
            stack: Vec::new(),
            base: 1, // for entry function
        }
    }
// ANCHOR_END: new

// ANCHOR: execute
    pub fn execute(&mut self, proto: &FuncProto) -> usize {
        let varargs = if proto.has_varargs {
            self.stack.drain(self.base + proto.nparam ..).collect()
        } else {
            Vec::new()
        };

        let mut pc = 0;
        loop {
            println!("  [{pc}]\t{:?}", proto.byte_codes[pc]);
            match proto.byte_codes[pc] {
// ANCHOR: vm_global
                ByteCode::GetGlobal(dst, name) => {
                    let name: &str = (&proto.constants[name as usize]).into();
                    let v = self.globals.get(name).unwrap_or(&Value::Nil).clone();
                    self.set_stack(dst, v);
                }
                ByteCode::SetGlobal(name, src) => {
                    let name = &proto.constants[name as usize];
                    let value = self.get_stack(src).clone();
                    self.globals.insert(name.into(), value);
                }
// ANCHOR_END: vm_global
                ByteCode::SetGlobalConst(name, src) => {
                    let name = &proto.constants[name as usize];
                    let value = proto.constants[src as usize].clone();
                    self.globals.insert(name.into(), value);
                }
                ByteCode::LoadConst(dst, c) => {
                    let v = proto.constants[c as usize].clone();
                    self.set_stack(dst, v);
                }
                ByteCode::LoadNil(dst, n) => {
                    self.fill_stack(dst as usize, n as usize);
                }
                ByteCode::LoadBool(dst, b) => {
                    self.set_stack(dst, Value::Boolean(b));
                }
                ByteCode::LoadInt(dst, i) => {
                    self.set_stack(dst, Value::Integer(i as i64));
                }
                ByteCode::Move(dst, src) => {
                    let v = self.get_stack(src).clone();
                    self.set_stack(dst, v);
                }
// ANCHOR: vm_table
                ByteCode::NewTable(dst, narray, nmap) => {
                    let table = Table::new(narray as usize, nmap as usize);
                    self.set_stack(dst, Value::Table(Rc::new(RefCell::new(table))));
                }
                ByteCode::SetInt(t, i, v) => {
                    let value = self.get_stack(v).clone();
                    self.set_table_int(t, i as i64, value);
                }
                ByteCode::SetIntConst(t, i, v) => {
                    let value = proto.constants[v as usize].clone();
                    self.set_table_int(t, i as i64, value);
                }
                ByteCode::SetField(t, k, v) => {
                    let key = proto.constants[k as usize].clone();
                    let value = self.get_stack(v).clone();
                    self.set_table(t, key, value);
                }
                ByteCode::SetFieldConst(t, k, v) => {
                    let key = proto.constants[k as usize].clone();
                    let value = proto.constants[v as usize].clone();
                    self.set_table(t, key, value);
                }
                ByteCode::SetTable(t, k, v) => {
                    let key = self.get_stack(k).clone();
                    let value = self.get_stack(v).clone();
                    self.set_table(t, key, value);
                }
                ByteCode::SetTableConst(t, k, v) => {
                    let key = self.get_stack(k).clone();
                    let value = proto.constants[v as usize].clone();
                    self.set_table(t, key, value);
                }
                ByteCode::SetList(table, n) => {
                    let ivalue = table as usize + 1;
                    if let Value::Table(table) = self.get_stack(table).clone() {
                        let values = self.stack.drain(ivalue .. ivalue + n as usize);
                        table.borrow_mut().array.extend(values);
                    } else {
                        panic!("not table");
                    }
                }
                ByteCode::GetInt(dst, t, k) => {
                    let value = self.get_table_int(t, k as i64);
                    self.set_stack(dst, value);
                }
                ByteCode::GetField(dst, t, k) => {
                    let key = &proto.constants[k as usize];
                    let value = self.get_table(t, key);
                    self.set_stack(dst, value);
                }
                ByteCode::GetTable(dst, t, k) => {
                    let key = self.get_stack(k);
                    let value = self.get_table(t, key);
                    self.set_stack(dst, value);
                }
// ANCHOR_END: vm_table

                // condition structures
                ByteCode::TestAndJump(icondition, jmp) => {
                    if self.get_stack(icondition).into() { // jump if true
                        pc = (pc as isize + jmp as isize) as usize;
                    }
                }
                ByteCode::TestOrJump(icondition, jmp) => {
                    if self.get_stack(icondition).into() {} else { // jump if false
                        pc = (pc as isize + jmp as isize) as usize;
                    }
                }
                ByteCode::TestAndSetJump(dst, icondition, jmp) => {
                    let condition = self.get_stack(icondition);
                    if condition.into() { // set and jump if true
                        self.set_stack(dst, condition.clone());
                        pc += jmp as usize;
                    }
                }
                ByteCode::TestOrSetJump(dst, icondition, jmp) => {
                    let condition = self.get_stack(icondition);
                    if condition.into() {} else { // set and jump if false
                        self.set_stack(dst, condition.clone());
                        pc += jmp as usize;
                    }
                }
                ByteCode::Jump(jmp) => {
                    pc = (pc as isize + jmp as isize) as usize;
                }

                // for-loop
// ANCHOR: for_prepare
                ByteCode::ForPrepare(dst, jmp) => {
                    // clear into 2 cases: integer and float
                    // stack: i, limit, step
                    if let (&Value::Integer(mut i), &Value::Integer(step)) =
                            (self.get_stack(dst), self.get_stack(dst + 2)) {
                        // integer case
                        if step == 0 {
                            panic!("0 step in numerical for");
                        }
                        let limit = match self.get_stack(dst + 1) {
                            &Value::Integer(limit) => limit,
                            &Value::Float(limit) => {
                                let limit = for_int_limit(limit, step>0, &mut i);
                                self.set_stack(dst+1, Value::Integer(limit));
                                limit
                            }
                            // TODO convert string
                            _ => panic!("invalid limit type"),
                        };
                        if !for_check(i, limit, step>0) {
                            pc += jmp as usize;
                        }
                    } else {
                        // float case
                        let i = self.make_float(dst);
                        let limit = self.make_float(dst+1);
                        let step = self.make_float(dst+2);
                        if step == 0.0 {
                            panic!("0 step in numerical for");
                        }
                        if !for_check(i, limit, step>0.0) {
                            pc += jmp as usize;
                        }
                    }
                }
// ANCHOR_END: for_prepare
                ByteCode::ForLoop(dst, jmp) => {
                    // stack: i, limit, step
                    match self.get_stack(dst) {
                        Value::Integer(i) => {
                            let limit = self.read_int(dst + 1);
                            let step = self.read_int(dst + 2);
                            let i = i + step;
                            if for_check(i, limit, step>0) {
                                self.set_stack(dst, Value::Integer(i));
                                pc -= jmp as usize;
                            }
                        }
                        Value::Float(f) => {
                            let limit = self.read_float(dst + 1);
                            let step = self.read_float(dst + 2);
                            let i = f + step;
                            if for_check(i, limit, step>0.0) {
                                self.set_stack(dst, Value::Float(i));
                                pc -= jmp as usize;
                            }
                        }
                        _ => panic!("xx"),
                    }
                }

                // function call
                ByteCode::Call(func, narg, want_nret) => {
                    let nret = self.call_function(func, narg);

                    // move return values to @func
                    self.stack.drain(self.base+func as usize .. self.stack.len()-nret);

                    // fill if need
                    if want_nret != MULTRET && nret < want_nret as usize {
                        self.fill_stack(nret, want_nret as usize - nret);
                    }
                }
                ByteCode::CallSet(dst, func, narg) => {
                    let nret = self.call_function(func, narg);

                    if nret == 0 {
                        self.set_stack(dst, Value::Nil);
                    } else {
                        if nret > 1 {
                            self.stack.truncate(self.stack.len() + 1 - nret);
                        }
                        // return value is at the last
                        self.stack.swap_remove(self.base + dst as usize);
                    }
                }
                ByteCode::Return(iret, nret) => {
                    let iret = self.base + iret as usize;
                    // move return values to function index
                    // self.stack signals all return values for MULTRET,
                    // so do not need truncate().
                    if nret != MULTRET {
                        self.stack.truncate(iret + nret as usize);
                    }
                    return nret as usize;
                }
                ByteCode::VarArgs(dst, want) => {
                    let (ncopy, need_fill) = if want == MULTRET {
                        (varargs.len(), 0)
                    } else if want as usize > varargs.len() {
                        (varargs.len(), want as usize - varargs.len())
                    } else {
                        (want as usize, 0)
                    };

                    for i in 0..ncopy {
                        self.set_stack(dst + i as u8, varargs[i].clone());
                    }
                    if need_fill > 0 {
                        self.fill_stack(dst as usize + ncopy, need_fill);
                    }
                }

                // unops
                ByteCode::Neg(dst, src) => {
                    let value = match &self.get_stack(src) {
                        Value::Integer(i) => Value::Integer(-i),
                        Value::Float(f) => Value::Float(-f),
                        _ => panic!("invalid -"),
                    };
                    self.set_stack(dst, value);
                }
                ByteCode::Not(dst, src) => {
                    let value = match &self.get_stack(src) {
                        Value::Nil => Value::Boolean(true),
                        Value::Boolean(b) => Value::Boolean(!b),
                        _ => Value::Boolean(false),
                    };
                    self.set_stack(dst, value);
                }
                ByteCode::BitNot(dst, src) => {
                    let value = match &self.get_stack(src) {
                        Value::Integer(i) => Value::Integer(!i),
                        _ => panic!("invalid ~"),
                    };
                    self.set_stack(dst, value);
                }
                ByteCode::Len(dst, src) => {
                    let value = match &self.get_stack(src) {
                        Value::ShortStr(len, _) => Value::Integer(*len as i64),
                        Value::MidStr(s) => Value::Integer(s.0 as i64),
                        Value::LongStr(s) => Value::Integer(s.len() as i64),
                        Value::Table(t) => Value::Integer(t.borrow().array.len() as i64),
                        _ => panic!("invalid -"),
                    };
                    self.set_stack(dst, value);
                }

                // binops
                ByteCode::Add(dst, a, b) => {
                    let r = exe_binop(&self.get_stack(a), &self.get_stack(b), |a,b|a+b, |a,b|a+b);
                    self.set_stack(dst, r);
                }
                ByteCode::AddConst(dst, a, b) => {
                    let r = exe_binop(&self.get_stack(a), &proto.constants[b as usize], |a,b|a+b, |a,b|a+b);
                    self.set_stack(dst, r);
                }
                ByteCode::AddInt(dst, a, i) => {
                    let r = exe_binop_int(&self.get_stack(a), i, |a,b|a+b, |a,b|a+b);
                    self.set_stack(dst, r);
                }
                ByteCode::Sub(dst, a, b) => {
                    let r = exe_binop(&self.get_stack(a), &self.get_stack(b), |a,b|a-b, |a,b|a-b);
                    self.set_stack(dst, r);
                }
                ByteCode::SubConst(dst, a, b) => {
                    let r = exe_binop(&self.get_stack(a), &proto.constants[b as usize], |a,b|a-b, |a,b|a-b);
                    self.set_stack(dst, r);
                }
                ByteCode::SubInt(dst, a, i) => {
                    let r = exe_binop_int(&self.get_stack(a), i, |a,b|a-b, |a,b|a-b);
                    self.set_stack(dst, r);
                }
                ByteCode::Mul(dst, a, b) => {
                    let r = exe_binop(&self.get_stack(a), &self.get_stack(b), |a,b|a*b, |a,b|a*b);
                    self.set_stack(dst, r);
                }
                ByteCode::MulConst(dst, a, b) => {
                    let r = exe_binop(&self.get_stack(a), &proto.constants[b as usize], |a,b|a*b, |a,b|a*b);
                    self.set_stack(dst, r);
                }
                ByteCode::MulInt(dst, a, i) => {
                    let r = exe_binop_int(&self.get_stack(a), i, |a,b|a*b, |a,b|a*b);
                    self.set_stack(dst, r);
                }
                ByteCode::Mod(dst, a, b) => {
                    let r = exe_binop(&self.get_stack(a), &self.get_stack(b), |a,b|a%b, |a,b|a%b);
                    self.set_stack(dst, r);
                }
                ByteCode::ModConst(dst, a, b) => {
                    let r = exe_binop(&self.get_stack(a), &proto.constants[b as usize], |a,b|a%b, |a,b|a%b);
                    self.set_stack(dst, r);
                }
                ByteCode::ModInt(dst, a, i) => {
                    let r = exe_binop_int(&self.get_stack(a), i, |a,b|a%b, |a,b|a%b);
                    self.set_stack(dst, r);
                }
                ByteCode::Idiv(dst, a, b) => {
                    let r = exe_binop(&self.get_stack(a), &self.get_stack(b), |a,b|a/b, |a,b|a/b);
                    self.set_stack(dst, r);
                }
                ByteCode::IdivConst(dst, a, b) => {
                    let r = exe_binop(&self.get_stack(a), &proto.constants[b as usize], |a,b|a/b, |a,b|a/b);
                    self.set_stack(dst, r);
                }
                ByteCode::IdivInt(dst, a, i) => {
                    let r = exe_binop_int(&self.get_stack(a), i, |a,b|a/b, |a,b|a/b);
                    self.set_stack(dst, r);
                }
                ByteCode::Div(dst, a, b) => {
                    let r = exe_binop_f(&self.get_stack(a), &self.get_stack(b), |a,b|a/b);
                    self.set_stack(dst, r);
                }
                ByteCode::DivConst(dst, a, b) => {
                    let r = exe_binop_f(&self.get_stack(a), &proto.constants[b as usize], |a,b|a/b);
                    self.set_stack(dst, r);
                }
                ByteCode::DivInt(dst, a, i) => {
                    let r = exe_binop_int_f(&self.get_stack(a), i, |a,b|a/b);
                    self.set_stack(dst, r);
                }
                ByteCode::Pow(dst, a, b) => {
                    let r = exe_binop_f(&self.get_stack(a), &self.get_stack(b), |a,b|a.powf(b));
                    self.set_stack(dst, r);
                }
                ByteCode::PowConst(dst, a, b) => {
                    let r = exe_binop_f(&self.get_stack(a), &proto.constants[b as usize], |a,b|a.powf(b));
                    self.set_stack(dst, r);
                }
                ByteCode::PowInt(dst, a, i) => {
                    let r = exe_binop_int_f(&self.get_stack(a), i, |a,b|a.powf(b));
                    self.set_stack(dst, r);
                }
                ByteCode::BitAnd(dst, a, b) => {
                    let r = exe_binop_i(&self.get_stack(a), &self.get_stack(b), |a,b|a&b);
                    self.set_stack(dst, r);
                }
                ByteCode::BitAndConst(dst, a, b) => {
                    let r = exe_binop_i(&self.get_stack(a), &proto.constants[b as usize], |a,b|a&b);
                    self.set_stack(dst, r);
                }
                ByteCode::BitAndInt(dst, a, i) => {
                    let r = exe_binop_int_i(&self.get_stack(a), i, |a,b|a&b);
                    self.set_stack(dst, r);
                }
                ByteCode::BitOr(dst, a, b) => {
                    let r = exe_binop_i(&self.get_stack(a), &self.get_stack(b), |a,b|a|b);
                    self.set_stack(dst, r);
                }
                ByteCode::BitOrConst(dst, a, b) => {
                    let r = exe_binop_i(&self.get_stack(a), &proto.constants[b as usize], |a,b|a|b);
                    self.set_stack(dst, r);
                }
                ByteCode::BitOrInt(dst, a, i) => {
                    let r = exe_binop_int_i(&self.get_stack(a), i, |a,b|a|b);
                    self.set_stack(dst, r);
                }
                ByteCode::BitXor(dst, a, b) => {
                    let r = exe_binop_i(&self.get_stack(a), &self.get_stack(b), |a,b|a^b);
                    self.set_stack(dst, r);
                }
                ByteCode::BitXorConst(dst, a, b) => {
                    let r = exe_binop_i(&self.get_stack(a), &proto.constants[b as usize], |a,b|a^b);
                    self.set_stack(dst, r);
                }
                ByteCode::BitXorInt(dst, a, i) => {
                    let r = exe_binop_int_i(&self.get_stack(a), i, |a,b|a^b);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftL(dst, a, b) => {
                    let r = exe_binop_i(&self.get_stack(a), &self.get_stack(b), |a,b|a<<b);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftLConst(dst, a, b) => {
                    let r = exe_binop_i(&self.get_stack(a), &proto.constants[b as usize], |a,b|a<<b);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftLInt(dst, a, i) => {
                    let r = exe_binop_int_i(&self.get_stack(a), i, |a,b|a<<b);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftR(dst, a, b) => {
                    let r = exe_binop_i(&self.get_stack(a), &self.get_stack(b), |a,b|a>>b);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftRConst(dst, a, b) => {
                    let r = exe_binop_i(&self.get_stack(a), &proto.constants[b as usize], |a,b|a>>b);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftRInt(dst, a, i) => {
                    let r = exe_binop_int_i(&self.get_stack(a), i, |a,b|a>>b);
                    self.set_stack(dst, r);
                }

                ByteCode::Equal(a, b, r) => {
                    if (self.get_stack(a) == self.get_stack(b)) == r {
                        pc += 1;
                    }
                }
                ByteCode::EqualConst(a, b, r) => {
                    if (self.get_stack(a) == &proto.constants[b as usize]) == r {
                        pc += 1;
                    }
                }
                ByteCode::EqualInt(a, i, r) => {
                    if let &Value::Integer(ii) = self.get_stack(a) {
                        if (ii == i as i64) == r {
                            pc += 1;
                        }
                    }
                }
                ByteCode::NotEq(a, b, r) => {
                    if (self.get_stack(a) != self.get_stack(b)) == r {
                        pc += 1;
                    }
                }
                ByteCode::NotEqConst(a, b, r) => {
                    if (self.get_stack(a) != &proto.constants[b as usize]) == r {
                        pc += 1;
                    }
                }
                ByteCode::NotEqInt(a, i, r) => {
                    if let &Value::Integer(ii) = self.get_stack(a) {
                        if (ii != i as i64) == r {
                            pc += 1;
                        }
                    }
                }
                ByteCode::LesEq(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(self.get_stack(b)).unwrap();
                    if !matches!(cmp, Ordering::Greater) == r {
                        pc += 1;
                    }
                }
                ByteCode::LesEqConst(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(&proto.constants[b as usize]).unwrap();
                    if !matches!(cmp, Ordering::Greater) == r {
                        pc += 1;
                    }
                }
                ByteCode::LesEqInt(a, i, r) => {
                    let a = match self.get_stack(a) {
                        &Value::Integer(i) => i,
                        &Value::Float(f) => f as i64,
                        _ => panic!("invalid compare"),
                    };
                    if (a <= i as i64) == r {
                        pc += 1;
                    }
                }
                ByteCode::GreEq(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(self.get_stack(b)).unwrap();
                    if !matches!(cmp, Ordering::Less) == r {
                        pc += 1;
                    }
                }
                ByteCode::GreEqConst(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(&proto.constants[b as usize]).unwrap();
                    if !matches!(cmp, Ordering::Less) == r {
                        pc += 1;
                    }
                }
                ByteCode::GreEqInt(a, i, r) => {
                    let a = match self.get_stack(a) {
                        &Value::Integer(i) => i,
                        &Value::Float(f) => f as i64,
                        _ => panic!("invalid compare"),
                    };
                    if (a >= i as i64) == r {
                        pc += 1;
                    }
                }
                ByteCode::Less(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(self.get_stack(b)).unwrap();
                    if matches!(cmp, Ordering::Less) == r {
                        pc += 1;
                    }
                }
                ByteCode::LessConst(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(&proto.constants[b as usize]).unwrap();
                    if matches!(cmp, Ordering::Less) == r {
                        pc += 1;
                    }
                }
                ByteCode::LessInt(a, i, r) => {
                    let a = match self.get_stack(a) {
                        &Value::Integer(i) => i,
                        &Value::Float(f) => f as i64,
                        _ => panic!("invalid compare"),
                    };
                    if (a < i as i64) == r {
                        pc += 1;
                    }
                }
                ByteCode::Greater(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(self.get_stack(b)).unwrap();
                    if matches!(cmp, Ordering::Greater) == r {
                        pc += 1;
                    }
                }
                ByteCode::GreaterConst(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(&proto.constants[b as usize]).unwrap();
                    if matches!(cmp, Ordering::Greater) == r {
                        pc += 1;
                    }
                }
                ByteCode::GreaterInt(a, i, r) => {
                    let a = match self.get_stack(a) {
                        &Value::Integer(i) => i,
                        &Value::Float(f) => f as i64,
                        _ => panic!("invalid compare"),
                    };
                    if (a > i as i64) == r {
                        pc += 1;
                    }
                }

                ByteCode::SetFalseSkip(dst) => {
                    self.set_stack(dst, Value::Boolean(false));
                    pc += 1;
                }

                ByteCode::Concat(dst, a, b) => {
                    let r = exe_concat(&self.get_stack(a), &self.get_stack(b));
                    self.set_stack(dst, r);
                }
                ByteCode::ConcatConst(dst, a, b) => {
                    let r = exe_concat(&self.get_stack(a), &proto.constants[b as usize]);
                    self.set_stack(dst, r);
                }
                ByteCode::ConcatInt(dst, a, i) => {
                    let r = exe_concat(&self.get_stack(a), &Value::Integer(i as i64));
                    self.set_stack(dst, r);
                }
            }

            pc += 1;
        }
    }
// ANCHOR_END: execute

    fn get_stack(&self, dst: u8) -> &Value {
        &self.stack[self.base + dst as usize]
    }
// ANCHOR: set_stack
    fn set_stack(&mut self, dst: u8, v: Value) {
        set_vec(&mut self.stack, self.base + dst as usize, v);
    }
// ANCHOR_END: set_stack
    fn fill_stack(&mut self, begin: usize, num: usize) {
        let begin = self.base + begin;
        let end = begin + num;
        let len = self.stack.len();
        if begin < len {
            self.stack[begin .. len].fill(Value::Nil);
        }
        if end > len {
            self.stack.resize(end, Value::Nil);
        }
    }

    fn set_table(&mut self, t: u8, key: Value, value: Value) {
        match &key {
            Value::Integer(i) => self.set_table_int(t, *i, value), // TODO Float
            _ => self.do_set_table(t, key, value),
        }
    }
    fn set_table_int(&mut self, t: u8, i: i64, value: Value) {
        if let Value::Table(table) = &self.get_stack(t) {
            let mut table = table.borrow_mut();
            // this is not same with Lua's official implement
            if i > 0 && (i < 4 || i < table.array.capacity() as i64 * 2) {
                set_vec(&mut table.array, i as usize - 1, value);
            } else {
                table.map.insert(Value::Integer(i), value);
            }
        } else {
            panic!("invalid table");
        }
    }
    fn do_set_table(&mut self, t: u8, key: Value, value: Value) {
        if let Value::Table(table) = &self.get_stack(t) {
            table.borrow_mut().map.insert(key, value);
        } else {
            panic!("invalid table");
        }
    }

    fn get_table(&self, t: u8, key: &Value) -> Value {
        match key {
            Value::Integer(i) => self.get_table_int(t, *i), // TODO Float
            _ => self.do_get_table(t, key),
        }
    }
    fn get_table_int(&self, t: u8, i: i64) -> Value {
        if let Value::Table(table) = &self.get_stack(t) {
            let table = table.borrow();
            table.array.get(i as usize - 1)
                .unwrap_or_else(|| table.map.get(&Value::Integer(i))
                    .unwrap_or(&Value::Nil)).clone()
        } else {
            panic!("set invalid table");
        }
    }
    fn do_get_table(&self, t: u8, key: &Value) -> Value {
        if let Value::Table(table) = &self.get_stack(t) {
            let table = table.borrow();
            table.map.get(key).unwrap_or(&Value::Nil).clone()
        } else {
            panic!("set invalid table");
        }
    }

    // call function
    // return the number of return values which are at the stack end
    fn call_function(&mut self, func: u8, narg: u8) -> usize {
        let fv = self.get_stack(func).clone();

        // get into new world, remember come back
        self.base += func as usize + 1;

        let narg = if narg == MULTRET {
            // self.stack signals all arguments
            self.stack.len() - self.base
        } else {
            narg as usize
        };

        let nret = match fv {
            Value::RustFunction(f) => {
                // drop potential temprary stack usage, to make sure get_top() works
                self.stack.truncate(self.base + narg);

                f(self) as usize
            }
            Value::LuaFunction(f) => {
                // fill missing arguments, but no need to truncate extras
                if narg < f.nparam {
                    self.fill_stack(narg, f.nparam - narg);
                }

                self.execute(&f)
            }
            v => panic!("invalid function: {v:?}"),
        };

        // come back
        self.base -= func as usize + 1;
        nret
    }

    fn make_float(&mut self, dst: u8) -> f64 {
        match self.get_stack(dst) {
            &Value::Float(f) => f,
            &Value::Integer(i) => {
                let f = i as f64;
                self.set_stack(dst, Value::Float(f));
                f
            }
            // TODO convert string
            ref v => panic!("not number {v:?}"),
        }
    }
    fn read_int(&self, dst: u8) -> i64 {
        if let &Value::Integer(i) = self.get_stack(dst) {
            i
        } else {
            panic!("invalid integer");
        }
    }
    fn read_float(&self, dst: u8) -> f64 {
        if let &Value::Float(f) = self.get_stack(dst) {
            f
        } else {
            panic!("invalid integer");
        }
    }
}

// API
impl<'a> ExeState {
    pub fn get_top(&self) -> usize {
        self.stack.len() - self.base
    }
    pub fn get_value(&self, i: usize) -> &Value {
        &self.stack[self.base + i - 1]
    }
    pub fn get<T>(&'a self, i: usize) -> T where T: From<&'a Value> {
        (&self.stack[self.base + i - 1]).into()
    }
}

fn set_vec(vec: &mut Vec<Value>, i: usize, value: Value) {
    match i.cmp(&vec.len()) {
        Ordering::Less => vec[i] = value,
        Ordering::Equal => vec.push(value),
        Ordering::Greater => {
            vec.resize(i, Value::Nil);
            vec.push(value);
        }
    }
}

fn exe_binop(v1: &Value, v2: &Value, arith_i: fn(i64,i64)->i64, arith_f: fn(f64,f64)->f64) -> Value {
    match (v1, v2) {
        (&Value::Integer(i1), &Value::Integer(i2)) => Value::Integer(arith_i(i1, i2)),
        (&Value::Integer(i1), &Value::Float(f2)) => Value::Float(arith_f(i1 as f64, f2)),
        (&Value::Float(f1), &Value::Float(f2)) => Value::Float(arith_f(f1, f2)),
        (&Value::Float(f1), &Value::Integer(i2)) => Value::Float(arith_f(f1, i2 as f64)),
        (_, _) => todo!("meta"),
    }
}
fn exe_binop_int(v1: &Value, i2: u8, arith_i: fn(i64,i64)->i64, arith_f: fn(f64,f64)->f64) -> Value {
    match v1 {
        &Value::Integer(i1) => Value::Integer(arith_i(i1, i2 as i64)),
        &Value::Float(f1) => Value::Float(arith_f(f1, i2 as f64)),
        _ => todo!("meta"),
    }
}

fn exe_binop_f(v1: &Value, v2: &Value, arith_f: fn(f64,f64)->f64) -> Value {
    let (f1, f2) = match (v1, v2) {
        (&Value::Integer(i1), &Value::Integer(i2)) => (i1 as f64, i2 as f64),
        (&Value::Integer(i1), &Value::Float(f2)) => (i1 as f64, f2),
        (&Value::Float(f1), &Value::Float(f2)) => (f1, f2),
        (&Value::Float(f1), &Value::Integer(i2)) => (f1, i2 as f64),
        (_, _) => todo!("meta"),
    };
    Value::Float(arith_f(f1, f2))
}
fn exe_binop_int_f(v1: &Value, i2: u8, arith_f: fn(f64,f64)->f64) -> Value {
    let f1 = match v1 {
        &Value::Integer(i1) => i1 as f64,
        &Value::Float(f1) => f1,
        _ => todo!("meta"),
    };
    Value::Float(arith_f(f1, i2 as f64))
}

fn exe_binop_i(v1: &Value, v2: &Value, arith_i: fn(i64,i64)->i64) -> Value {
    let (i1, i2) = match (v1, v2) {
        (&Value::Integer(i1), &Value::Integer(i2)) => (i1, i2),
        (&Value::Integer(i1), &Value::Float(f2)) => (i1, ftoi(f2).unwrap()),
        (&Value::Float(f1), &Value::Float(f2)) => (ftoi(f1).unwrap(), ftoi(f2).unwrap()),
        (&Value::Float(f1), &Value::Integer(i2)) => (ftoi(f1).unwrap(), i2),
        (_, _) => todo!("meta"),
    };
    Value::Integer(arith_i(i1, i2))
}
fn exe_binop_int_i(v1: &Value, i2: u8, arith_i: fn(i64,i64)->i64) -> Value {
    let i1 = match v1 {
        &Value::Integer(i1) => i1,
        &Value::Float(f1) => ftoi(f1).unwrap(),
        _ => todo!("meta"),
    };
    Value::Integer(arith_i(i1, i2 as i64))
}

fn exe_concat(v1: &Value, v2: &Value) -> Value {
    // TODO remove duplicated code
    let mut numbuf1: Vec<u8> = Vec::new();
    let v1 = match v1 {
        Value::Integer(i) => {
            write!(&mut numbuf1, "{}", i).unwrap();
            numbuf1.as_slice()
        }
        Value::Float(f) => {
            write!(&mut numbuf1, "{}", f).unwrap();
            numbuf1.as_slice()
        }
        _ => v1.into()
    };

    let mut numbuf2: Vec<u8> = Vec::new();
    let v2 = match v2 {
        Value::Integer(i) => {
            write!(&mut numbuf2, "{}", i).unwrap();
            numbuf2.as_slice()
        }
        Value::Float(f) => {
            write!(&mut numbuf2, "{}", f).unwrap();
            numbuf2.as_slice()
        }
        _ => v2.into()
    };

    [v1, v2].concat().into()
}

fn for_check<T: PartialOrd>(i: T, limit: T, is_step_positive: bool) -> bool {
    if is_step_positive {
        i <= limit
    } else {
        i >= limit
    }
}

fn for_int_limit(limit: f64, is_step_positive: bool, i: &mut i64) -> i64 {
    if is_step_positive {
        if limit < i64::MIN as f64 {
            // The limit is so negative that the for-loop should not run,
            // because any initial integer value is greater than such limit.
            // If we do not handle this case specially and return (limit as i64)
            // as normal, which will be converted into i64::MIN, and if the
            // initial integer is i64::MIN too, then the loop will run once,
            // which is wrong!
            // So we reset the initial integer to 0 and return limit as -1,
            // to make sure the loop must not be run.
            *i = 0;
            -1
        } else {
            limit.floor() as i64
        }
    } else {
        if limit > i64::MAX as f64 {
            *i = 0;
            1
        } else {
            limit.ceil() as i64
        }
    }
}