//! Node kinds of the syntax tree. Tokens keep the lexer's `sv_lexer::Kind`.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    /// A marker the parser started and then dropped; never in a built tree.
    Tombstone,
    /// Tokens the parser could not place.
    Error,

    SourceFile,
    /// `(* ... *)`
    Attribute,
    /// The name of a declaration: one identifier, or a scoped name such as
    /// `uvm_object::print` for an out-of-block method.
    Name,
    /// `: name` after an end keyword.
    EndLabel,
    /// Everything between a declaration keyword and its `;` that later
    /// slices will parse further (parameter and port lists, extends, ...).
    Header,
    /// A parenthesized list kept as written for now (ports, arguments).
    ParenList,
    /// `#( ... )` parameter list or assignment.
    ParamList,

    // Design units and containers.
    ModuleDecl,
    InterfaceDecl,
    ProgramDecl,
    CheckerDecl,
    PackageDecl,
    ClassDecl,
    GenerateRegion,
    GenerateBlock,
    LoopGenerate,
    IfGenerate,
    CaseGenerate,

    // Items.
    FunctionDecl,
    TaskDecl,
    /// `extern` or `pure virtual` method, no body.
    MethodPrototype,
    /// Return type of a function, as written.
    ReturnType,
    TypedefDecl,
    /// `typedef class C;`
    ForwardTypedef,
    ConstraintDecl,
    /// `constraint c;` / `extern constraint c;`
    ConstraintPrototype,
    /// `{ ... }` of a constraint, kept as written.
    ConstraintBlock,
    CovergroupDecl,
    PropertyDecl,
    SequenceDecl,
    ClockingDecl,
    ImportDecl,
    ExportDecl,
    DpiImport,
    DpiExport,
    ParamDecl,
    /// A data, net or class-property declaration, or a module
    /// instantiation (they are told apart in a later slice).
    DataDecl,
    ContinuousAssign,
    /// always*, initial, final with its statement.
    ProceduralBlock,
    TimeunitDecl,
    GenvarDecl,
    ModportDecl,
    BindDirective,
    LetDecl,
    /// config, primitive, specify and other blocks kept as written.
    OtherBlock,
    /// Any other item, up to its `;`.
    OtherItem,
    EmptyItem,

    // Statements.
    /// The statements of a function or task.
    Body,
    SeqBlock,
    ParBlock,
    IfStmt,
    ElseClause,
    CaseStmt,
    LoopStmt,
    DoWhileStmt,
    /// `#d stmt`, `@(e) stmt`, `##n stmt`.
    TimedStmt,
    WaitStmt,
    AssertStmt,
    RandSequenceStmt,
    /// `label:` in front of a statement.
    StmtLabel,
    EmptyStmt,
    /// Any other statement or a local declaration, up to its `;`.
    SimpleStmt,
}
