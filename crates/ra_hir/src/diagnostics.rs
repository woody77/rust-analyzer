//! FIXME: write short doc here
pub use hir_def::diagnostics::UnresolvedModule;
pub use hir_expand::diagnostics::{AstDiagnostic, Diagnostic, DiagnosticSink};
pub use hir_ty::diagnostics::{
    MismatchedArgCount, MissingFields, MissingMatchArms, MissingOkInTailExpr, NoSuchField,
};
