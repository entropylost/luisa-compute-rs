use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::{spanned::Spanned, Attribute, Item, ItemEnum, ItemFn, ItemStruct, NestedMeta};
pub struct Compiler {
    inside_crate: bool,
}
impl Compiler {
    fn crate_path(&self) -> TokenStream {
        if self.inside_crate {
            quote!(crate::lang)
        } else {
            quote!(luisa_compute::lang)
        }
    }
    pub fn new(inside_crate: bool) -> Self {
        Self { inside_crate }
    }
    pub fn compile_fn(&self, args: &Vec<NestedMeta>, func: &ItemFn) -> TokenStream {
        quote!(#func)
    }
    pub fn compile_kernel(&self, func: &ItemFn) -> TokenStream {
        todo!()
    }
    fn check_repr_c(&self, attribtes: &Vec<Attribute>) {
        let mut has_repr_c = false;
        for attr in attribtes {
            let path = attr.path.get_ident().unwrap().to_string();
            if path == "repr" {
                let tokens = attr.tokens.to_string();
                if tokens == "(C)" {
                    has_repr_c = true;
                }
            }
        }
        if !has_repr_c {
            panic!("Struct must have repr(C) attribute");
        }
    }
    pub fn derive_value(&self, struct_: &ItemStruct) -> TokenStream {
        self.check_repr_c(&struct_.attrs);
        let span = struct_.span();
        let crate_path = self.crate_path();
        let name = &struct_.ident;
        let vis = &struct_.vis;
        let fields: Vec<_> = struct_.fields.iter().map(|f| f).collect();
        let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();
        let expr_proxy_field_methods: Vec<_> = fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let ident = f.ident.as_ref().unwrap();
                let vis = &f.vis;
                let ty = &f.ty;
                quote_spanned!(span=>
                    #vis fn #ident (&self) -> Expr<#ty> {
                        <Expr::<#ty> as ExprProxy<#ty>>::from_node(__extract::<#ty>(
                            self.node, #i,
                        ))
                    }
                )
            })
            .collect();
        let var_proxy_field_methods: Vec<_> = fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let ident = f.ident.as_ref().unwrap();
                let vis = &f.vis;
                let ty = &f.ty;
                quote_spanned!(span=>
                    #vis fn #ident (&self) -> Var<#ty> {
                        <Var::<#ty> as VarProxy<#ty>>::from_node(__extract::<#ty>(
                            self.node, #i,
                        ))
                    }
                )
            })
            .collect();
        let expr_proxy_name = syn::Ident::new(&format!("{}Expr", name), name.span());
        let var_proxy_name = syn::Ident::new(&format!("{}Var", name), name.span());
        let type_of_impl = quote_spanned!(span=>
            impl #crate_path ::TypeOf for #name {
                fn type_() ->  #crate_path ::Gc< #crate_path ::Type> {
                    use #crate_path ::*;
                    let size = std::mem::size_of::<#name>();
                    let alignment = std::mem::align_of::<#name>();
                    let struct_type = StructType {
                        fields: CBoxedSlice::new(vec![#(<#field_types as TypeOf>::type_(),)*]),
                        size,
                        alignment
                    };
                    let type_ = Type::Struct(struct_type);
                    register_type(type_)
                }
            }
        );
        let proxy_def = quote_spanned!(span=>
            #[derive(Clone, Copy, Debug)]
            #vis struct #expr_proxy_name {
                node: #crate_path ::NodeRef,
            }
            #[derive(Clone, Copy, Debug)]
            #vis struct #var_proxy_name {
                node: #crate_path ::NodeRef,
            }
            impl #crate_path ::Aggregate for #expr_proxy_name {
                fn to_nodes(&self, nodes: &mut Vec<#crate_path ::NodeRef>) {
                    nodes.push(self.node);
                }
                fn from_nodes<I: Iterator<Item = #crate_path ::NodeRef>>(iter: &mut I) -> Self {
                    Self{
                        node: iter.next().unwrap()
                    }
                }
            }
            impl #crate_path ::Aggregate for #var_proxy_name {
                fn to_nodes(&self, nodes: &mut Vec<#crate_path ::NodeRef>) {
                    nodes.push(self.node);
                }
                fn from_nodes<I: Iterator<Item = #crate_path ::NodeRef>>(iter: &mut I) -> Self {
                    Self{
                        node: iter.next().unwrap()
                    }
                }
            }
            impl #crate_path ::ExprProxy<#name> for #expr_proxy_name {
                #[allow(unused_assignments)]
                fn from_node(node: #crate_path ::NodeRef) -> Self {
                    Self { node }
                }
                fn node(&self) -> #crate_path ::NodeRef {
                    self.node
                }
            }
            impl #crate_path ::VarProxy<#name> for #var_proxy_name {
                #[allow(unused_assignments)]
                fn from_node(node: #crate_path ::NodeRef) -> Self {
                    Self { node }
                }
                fn node(&self) -> #crate_path ::NodeRef {
                    self.node
                }
            }
            impl From<#var_proxy_name> for #expr_proxy_name {
                fn from(var: #var_proxy_name) -> Self {
                    var.load()
                }
            }
        );
        quote_spanned! {
            span=>
            #proxy_def
            #type_of_impl
            impl #crate_path ::Value for #name {
                type Expr = #expr_proxy_name;
                type Var = #var_proxy_name;
            }
            impl #expr_proxy_name {
                #(#expr_proxy_field_methods)*
            }
            impl #var_proxy_name {
                #(#var_proxy_field_methods)*
            }
        }
    }
    pub fn derive_aggregate_for_struct(&self, struct_: &ItemStruct) -> TokenStream {
        todo!()
    }
    pub fn derive_aggregate_for_enum(&self, enum_: &ItemEnum) -> TokenStream {
        todo!()
    }
    pub fn derive_aggregate(&self, item: &Item) -> TokenStream {
        match item {
            Item::Struct(struct_) => self.derive_aggregate_for_struct(struct_),
            Item::Enum(enum_) => self.derive_aggregate_for_enum(enum_),
            _ => todo!(),
        }
    }
}
