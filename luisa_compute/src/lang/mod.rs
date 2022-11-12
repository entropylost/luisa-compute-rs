use std::{any::Any, collections::HashMap, ops::Deref, sync::Arc};

use crate::{
    resource::{BindlessArrayHandle, Buffer, BufferHandle, TextureHandle},
    *,
};
pub use ir::ir::NodeRef;
use ir::{
    ir::{new_node, BasicBlock, Const, Func, Instruction, IrBuilder, Node, Type},
    CBoxedSlice,
};
use luisa_compute_ir as ir;
use luisa_compute_ir::TypeOf;
use std::cell::RefCell;
pub mod math;
pub mod math_impl;
pub mod traits;
pub mod traits_impl;

pub trait Value: Copy + ir::TypeOf {
    type Proxy: VarProxy<Self>;
}

pub trait Aggregate: Sized {
    fn to_vec_nodes(&self) -> Vec<NodeRef> {
        let mut nodes = vec![];
        Self::to_nodes(&self, &mut nodes);
        nodes
    }
    fn from_vec_nodes(nodes: Vec<NodeRef>) -> Self {
        let mut iter = nodes.into_iter();
        let ret = Self::from_nodes(&mut iter);
        assert!(iter.next().is_none());
        ret
    }
    fn to_nodes(&self, nodes: &mut Vec<NodeRef>);
    fn from_nodes<I: Iterator<Item = NodeRef>>(iter: &mut I) -> Self;
    fn store(&self, value: &Self) {
        let value_nodes = value.to_vec_nodes();
        let self_nodes = self.to_vec_nodes();
        assert_eq!(value_nodes.len(), self_nodes.len());
        current_scope(|b| {
            for (value_node, self_node) in value_nodes.into_iter().zip(self_nodes.into_iter()) {
                b.store(self_node, value_node);
            }
        })
    }
}
pub trait Selectable {
    fn select(mask: Mask, lhs: Self, rhs: Self) -> Self;
}
pub trait VarProxy<T>: Copy + From<T> + Aggregate {
    fn from_node(node: NodeRef) -> Self;
    fn node(&self) -> NodeRef;
}
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Var<T: Value> {
    pub(crate) proxy: T::Proxy,
}
pub type Mask = Var<bool>;

impl<T: Value> Var<T> {
    pub fn store(&self, value: Self) {
        self.proxy.store(&value.proxy);
    }
    pub(crate) fn expand(&self) -> Vec<NodeRef> {
        self.proxy.to_vec_nodes()
    }
    pub(crate) fn collect(nodes: &[NodeRef]) -> Self {
        let proxy = T::Proxy::from_nodes(&mut nodes.iter().cloned());
        Self { proxy }
    }
    pub(crate) fn from_node(node: NodeRef) -> Self {
        Self {
            proxy: T::Proxy::from_node(node),
        }
    }
    pub(crate) fn node(&self) -> NodeRef {
        self.proxy.node()
    }
}
impl<T: Value> From<T> for Var<T> {
    fn from(t: T) -> Self {
        Self {
            proxy: T::Proxy::from(t),
        }
    }
}

impl<T: Value> Deref for Var<T> {
    type Target = T::Proxy;
    fn deref(&self) -> &Self::Target {
        &self.proxy
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PrimProxy<T> {
    pub(crate) node: NodeRef,
    pub(crate) _phantom: std::marker::PhantomData<T>,
}

impl<T> Aggregate for PrimProxy<T> {
    fn to_nodes(&self, nodes: &mut Vec<NodeRef>) {
        nodes.push(self.node);
    }
    fn from_nodes<I: Iterator<Item = NodeRef>>(iter: &mut I) -> Self {
        Self {
            node: iter.next().unwrap(),
            _phantom: std::marker::PhantomData,
        }
    }
}
macro_rules! impl_prim {
    ($t:ty) => {
        impl From<$t> for PrimProxy<$t> {
            fn from(v: $t) -> Self {
                const_(v).proxy
            }
        }
        impl VarProxy<$t> for PrimProxy<$t> {
            fn from_node(node: NodeRef) -> Self {
                Self {
                    node,
                    _phantom: std::marker::PhantomData,
                }
            }
            fn node(&self) -> NodeRef {
                self.node
            }
        }
        impl Value for $t {
            type Proxy = PrimProxy<$t>;
        }
    };
}

impl_prim!(bool);
impl_prim!(u32);
impl_prim!(u64);
impl_prim!(i32);
impl_prim!(i64);
impl_prim!(f32);
// impl_prim!(f64);

pub type Bool = Var<bool>;
pub type Float = Var<f32>;
pub type Int = Var<i32>;
pub type Uint = Var<u32>;
pub type Long = Var<i64>;
pub type Ulong = Var<u64>;

pub(crate) struct Recorder {
    scopes: Vec<IrBuilder>,
    buffer_to_node: HashMap<u64, NodeRef>,
}

thread_local! {
    pub(crate) static RECORDER: RefCell<Recorder> = RefCell::new(Recorder {
        scopes: vec![],
        buffer_to_node: HashMap::new(),
    });
}

// Don't call this function directly unless you know what you are doing
pub fn current_scope<F: FnOnce(&mut IrBuilder) -> R, R>(f: F) -> R {
    RECORDER.with(|r| {
        let mut r = r.borrow_mut();
        let s = &mut r.scopes;
        if s.is_empty() {
            s.push(IrBuilder::new());
        }
        f(s.last_mut().unwrap())
    })
}
// Don't call this function directly unless you know what you are doing
pub fn pop_scope() -> Gc<BasicBlock> {
    RECORDER.with(|r| {
        let mut r = r.borrow_mut();
        let s = &mut r.scopes;
        s.pop().unwrap().finish()
    })
}
pub fn __extract<T: Value>(node: NodeRef, index: usize) -> NodeRef {
    current_scope(|b| {
        let i = b.const_(Const::Int32(index as i32));
        let node = b.call(Func::ExtractElement, &[node, i], <T as TypeOf>::type_());
        node
    })
}
pub fn __compose<T: Value>(nodes: &[NodeRef]) -> NodeRef {
    current_scope(|b| b.call(Func::Struct, nodes, <T as TypeOf>::type_()))
}
pub fn const_<T: Value + Copy + 'static>(value: T) -> Var<T> {
    let node = current_scope(|s| -> NodeRef {
        let any = &value as &dyn Any;
        if let Some(value) = any.downcast_ref::<bool>() {
            s.const_(Const::Bool(*value))
        } else if let Some(value) = any.downcast_ref::<i32>() {
            s.const_(Const::Int32(*value))
        } else if let Some(value) = any.downcast_ref::<u32>() {
            s.const_(Const::Uint32(*value))
        } else if let Some(value) = any.downcast_ref::<i64>() {
            s.const_(Const::Int64(*value))
        } else if let Some(value) = any.downcast_ref::<u64>() {
            s.const_(Const::Uint64(*value))
        } else if let Some(value) = any.downcast_ref::<f32>() {
            s.const_(Const::Float32(*value))
        } else if let Some(value) = any.downcast_ref::<f64>() {
            s.const_(Const::Float64(*value))
        } else {
            let mut buf = vec![0u8; std::mem::size_of::<T>()];
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &value as *const T as *const u8,
                    buf.as_mut_ptr(),
                    buf.len(),
                );
            }
            s.const_(Const::Generic(CBoxedSlice::new(buf), T::type_()))
        }
    });
    Var::from_node(node)
}

pub struct BufferVar<T: Value> {
    marker: std::marker::PhantomData<T>,
    #[allow(dead_code)]
    handle: Arc<BufferHandle>,
    node: NodeRef,
}

impl<T: Value> Drop for BufferVar<T> {
    fn drop(&mut self) {
        todo!()
    }
}
pub struct BindlessArrayVar {
    #[allow(dead_code)]
    handle: Arc<BindlessArrayHandle>,
    node: NodeRef,
}
impl BindlessArrayVar {
    pub fn buffer_read<T: Value, BI: Into<Uint>, EI: Into<Uint>>(
        &self,
        buffer_index: BI,
        element_index: EI,
    ) -> Var<T> {
        Var::from_node(current_scope(|b| {
            b.call(
                Func::BindlessBufferRead,
                &[
                    self.node,
                    buffer_index.into().node(),
                    element_index.into().node(),
                ],
                T::type_(),
            )
        }))
    }
    pub fn buffer_length<I: Into<Uint>>(&self, buffer_index: I) -> Uint {
        Uint::from_node(current_scope(|b| {
            b.call(
                Func::BindlessBufferSize,
                &[self.node, buffer_index.into().node()],
                u32::type_(),
            )
        }))
    }
}
impl<T: Value> BufferVar<T> {
    pub fn new(buffer: &Buffer<T>) -> Self {
        let node = RECORDER.with(|r| {
            let mut r = r.borrow_mut();
            let handle: u64 = buffer.handle().0;
            if let Some(node) = r.buffer_to_node.get(&handle) {
                *node
            } else {
                let node = new_node(Node::new(Gc::new(Instruction::Buffer), T::type_()));
                r.buffer_to_node.insert(handle, node);
                node
            }
        });
        Self {
            node,
            marker: std::marker::PhantomData,
            handle: buffer.handle.clone(),
        }
    }
    pub fn len(&self) -> Uint {
        Var::from_node(
            current_scope(|b| b.call(Func::BufferSize, &[self.node], u32::type_())).into(),
        )
    }
    pub fn read<I: Into<Uint>>(&self, i: I) -> Var<T> {
        current_scope(|b| {
            Var::from_node(b.call(Func::BufferRead, &[self.node, i.into().node()], T::type_()))
        })
    }
    pub fn write<I: Into<Uint>, V: Into<Var<T>>>(&self, i: I, v: V) {
        current_scope(|b| {
            b.call(
                Func::BufferWrite,
                &[self.node, i.into().node(), v.into().node()],
                Type::void(),
            )
        });
    }
    pub fn atomic_exchange<I: Into<Uint>, V: Into<Var<T>>>(&self, i: I, v: V) -> Var<T> {
        todo!()
    }
}

pub struct ImageVar<T: Value> {
    #[allow(dead_code)]
    handle: Arc<TextureHandle>,
    _marker: std::marker::PhantomData<T>,
}

pub struct VolumeVar<T: Value> {
    #[allow(dead_code)]
    handle: Arc<TextureHandle>,
    _marker: std::marker::PhantomData<T>,
}
pub type Tex2DVar<T> = ImageVar<T>;
pub type Tex3DVar<T> = VolumeVar<T>;
