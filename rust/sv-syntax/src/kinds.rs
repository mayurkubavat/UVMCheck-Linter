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
    /// An expression statement: assignment, call, increment.
    ExprStmt,
    ReturnStmt,
    /// `break;` / `continue;`
    JumpStmt,
    DisableStmt,
    /// `-> e;`
    EventTrigger,
    /// Procedural assign/deassign/force/release.
    ProcAssign,
    /// `assert property`, `cover sequence`, `expect`: kept as written.
    ConcurrentAssert,
    /// `#d`, `@(e)`, `##n`: kept as written.
    TimingControl,

    // Declarations.
    DataType,
    /// `[...]` packed or unpacked dimension.
    Dim,
    /// A declared name with its dimensions and initializer.
    Declarator,
    /// A module or interface instantiation.
    Instantiation,
    /// One instance: name, dimensions, port connections.
    Instance,
    /// Gate or switch primitive instantiation.
    GateInst,

    // Headers and slice 3a.
    /// `extends ...` / `implements ...` of a class.
    Extends,
    PortList,
    Port,
    /// `[edge] expr [iff expr]` in an event control.
    EventExpr,
    ConstraintItem,
    /// `expr dist { ... }`
    ExprOrDist,
    DistItem,
    /// `@@(begin f)`
    BlockEvent,
    /// `option.x = v;`
    CoverageOption,
    Coverpoint,
    Cross,
    Bins,
    TransList,
    TransItem,
    BinsSelect,

    // Slice 3b.
    /// `disable iff (expr)`
    DisableIff,
    /// A property specification: clocking, disable iff, expression.
    PropertyExpr,
    /// A sequence or property expression or one of its operands.
    SequenceExpr,
    /// `[*n]`, `[->n]`, `[=n]` or a cycle range.
    Repetition,
    ClockingItem,
    /// `default disable iff expr;`
    DefaultDisable,

    // Expressions.
    BinaryExpr,
    PrefixExpr,
    PostfixExpr,
    CondExpr,
    InsideExpr,
    /// `{ ... }` after `inside`.
    RangeList,
    /// `[lo : hi]` in a range list or case item.
    OpenRange,
    ParenExpr,
    Literal,
    /// An identifier, system name, `this`, `super`, `null`, `$`.
    NameExpr,
    /// `a::b`
    ScopedExpr,
    /// `a.b`
    MemberExpr,
    /// `a[i]`, `a[i:j]`, `a[i+:w]`
    SelectExpr,
    /// `f(args)`, also `x.m() with (...)`.
    CallExpr,
    ArgList,
    Arg,
    /// `with (...)` / `with { ... }` after a method call.
    WithClause,
    ConcatExpr,
    /// `'{ ... }`
    AssignPattern,
    /// `T'(x)`, `8'(x)`, `T'{...}`
    CastExpr,
    NewExpr,
    TaggedExpr,
    /// `x matches pattern`, `a &&& b` in a condition.
    MatchExpr,
    Pattern,
    /// `#1 x` inside an assignment.
    TimedExpr,
    /// A type standing where an expression may: `$bits(int)`.
    TypeExpr,
}
