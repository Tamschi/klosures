#![doc(html_root_url = "https://docs.rs/klosures/0.0.1")]
#![warn(clippy::pedantic)]
#![allow(clippy::single_match)]

use proc_macro::TokenStream as TokenStream1;
use quote::{quote_spanned, ToTokens};
use smartstring::alias::String;
use std::{collections::HashSet, iter};
use syn::{
	parse_macro_input,
	punctuated::{Pair, Punctuated},
	visit_mut::{self, VisitMut},
	Expr, ExprCall, ExprClosure, ExprMethodCall, ExprParen, Ident, ItemFn, LitStr, Pat, PatIdent,
	ReturnType, Token,
};

mod kw {
	syn::custom_keyword!(degroup);
}

#[proc_macro_attribute]
pub fn klosures(attr: TokenStream1, item: TokenStream1) -> TokenStream1 {
	let degroup = parse_macro_input!(attr as Option<kw::degroup>).is_some();
	let mut function = parse_macro_input!(item as ItemFn);
	ItemFnVisitor::new(degroup).visit_item_fn_mut(&mut function);
	function.to_token_stream().into()
}

struct ItemFnVisitor {
	degroup: bool,
	nested: bool,
}
impl ItemFnVisitor {
	pub fn new(degroup: bool) -> Self {
		Self {
			degroup,
			nested: false,
		}
	}
}
impl VisitMut for ItemFnVisitor {
	fn visit_item_fn_mut(&mut self, ast: &mut ItemFn) {
		if self.nested {
			// Stop.
		} else {
			self.nested = true;
			visit_mut::visit_item_fn_mut(self, ast);
			self.nested = false;
		}
	}

	fn visit_expr_call_mut(&mut self, ast: &mut ExprCall) {
		visit_mut::visit_expr_call_mut(self, ast);
		for arg in &mut ast.args.iter_mut() {
			ParamVisitor::new(self.degroup).visit_expr_mut(arg)
		}
	}

	fn visit_expr_method_call_mut(&mut self, ast: &mut ExprMethodCall) {
		visit_mut::visit_expr_method_call_mut(self, ast);
		for arg in &mut ast.args.iter_mut() {
			ParamVisitor::new(self.degroup).visit_expr_mut(arg)
		}
	}
}

struct ParamVisitor {
	degroup: bool,
	nested: bool,
	its: HashSet<Ident>,
	ignore: HashSet<Ident>,
}
impl ParamVisitor {
	pub fn new(degroup: bool) -> Self {
		Self {
			degroup,
			nested: false,
			its: HashSet::new(),
			ignore: HashSet::new(),
		}
	}
}

impl VisitMut for ParamVisitor {
	fn visit_item_fn_mut(&mut self, _: &mut ItemFn) {
		// Stop.
	}

	fn visit_expr_closure_mut(&mut self, ast: &mut ExprClosure) {
		let mut ignored = HashSet::new();
		for input in &ast.inputs {
			match input {
				syn::Pat::Ident(PatIdent { ident, .. }) => {
					if self.ignore.insert(ident.clone()) {
						ignored.insert(ident.clone());
					}
				}
				_ => (
					// This macro only produces plain ident patterns, ignore anything else.
				),
			}
		}
		visit_mut::visit_expr_closure_mut(self, ast);
		for ident in ignored {
			assert!(self.ignore.remove(&ident))
		}
	}

	fn visit_expr_mut(&mut self, ast: &mut Expr) {
		if self.nested || self.degroup && matches!(ast, Expr::Group(_)) {
			visit_mut::visit_expr_mut(self, ast)
		} else {
			take_mut::take(ast, |ast| match ast {
				Expr::Paren(ExprParen {
					attrs,
					paren_token,
					mut expr,
				}) => {
					// These extra parens are our special marker.
					self.nested = true;
					visit_mut::visit_expr_mut(self, &mut *expr);
					self.nested = false;
					Expr::Closure(ExprClosure {
						attrs,
						asyncness: None,
						movability: None,
						capture: None,
						or1_token: Token![|](paren_token.span),
						inputs: match self.its.len() {
							0..=1 => self
								.its
								.drain()
								.map(|it| {
									Pat::Ident(PatIdent {
										attrs: vec![],
										by_ref: None,
										mutability: None,
										ident: it,
										subpat: None,
									})
								})
								.collect(),
							n => {
								let mut input = Punctuated::<Pat, Token![,]>::new();
								for i in 0..n {
									let mut name = String::from("it");
									itoa::fmt(&mut name, i).unwrap();
									let it = match self
										.its
										.iter()
										.find(|it| *it == name.as_str())
										.cloned()
									{
										None => {
											let message = LitStr::new(
												&format!(
													"Missing expected identifier in sequence: `{}`",
													name
												),
												paren_token.span,
											);
											Pat::Verbatim(
												quote_spanned!(paren_token.span=> ::core::compile_error!(#message)),
											)
										}
										Some(it) => {
											let it = self.its.take(&it).unwrap();
											Pat::Ident(PatIdent {
												attrs: vec![],
												by_ref: None,
												mutability: None,
												ident: it,
												subpat: None,
											})
										}
									};
									input.extend(iter::once(Pair::Punctuated(
										it,
										Token![,](paren_token.span),
									)))
								}
								assert!(self.its.is_empty());
								input
							}
						},
						or2_token: Token![|](paren_token.span),
						output: ReturnType::Default,
						body: expr,
					})
				}
				_ => ast, // Stop.
			})
		}
	}

	fn visit_expr_call_mut(&mut self, ast: &mut ExprCall) {
		self.visit_expr_mut(&mut *ast.func);
		for arg in ast.args.iter_mut() {
			self.visit_expr_mut(arg)
		}
	}

	fn visit_expr_method_call_mut(&mut self, ast: &mut ExprMethodCall) {
		self.visit_expr_mut(&mut *ast.receiver);
		for arg in ast.args.iter_mut() {
			self.visit_expr_mut(arg)
		}
	}

	fn visit_ident_mut(&mut self, ast: &mut Ident) {
		if !self.ignore.contains(&ast) && {
			let name = ast.to_string();
			name.starts_with("it") && name["it".len()..].chars().all(|c| c.is_ascii_digit())
		} {
			self.its.insert(ast.clone());
		}
	}
}
