//! Node kinds of the syntax tree. Tokens keep the lexer's `sv_lexer::Kind`.

/// The kind of a syntax tree node. Variants are named after the IEEE 1800
/// construct they hold; a node's children are its tokens and sub-nodes in
/// source order (see `Tree`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    /// A marker the parser started and then dropped; never in a built tree.
    Tombstone,
    /// Tokens the parser could not place.
    Error,

    /// The root: every item of a compilation unit, in order.
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
    /// `module ... endmodule` (also `macromodule`).
    ModuleDecl,
    /// `interface ... endinterface`.
    InterfaceDecl,
    /// `program ... endprogram`.
    ProgramDecl,
    /// `checker ... endchecker`.
    CheckerDecl,
    /// `package ... endpackage`.
    PackageDecl,
    /// `class ... endclass`, `interface class` included.
    ClassDecl,
    /// `generate ... endgenerate`.
    GenerateRegion,
    /// A `begin ... end` block in a generate construct.
    GenerateBlock,
    /// A generate `for` loop.
    LoopGenerate,
    /// A generate `if`/`else`.
    IfGenerate,
    /// A generate `case`.
    CaseGenerate,

    // Items.
    /// A function with its body.
    FunctionDecl,
    /// A task with its body.
    TaskDecl,
    /// `extern` or `pure virtual` method, no body.
    MethodPrototype,
    /// Return type of a function, as written.
    ReturnType,
    /// `typedef <type> name;`
    TypedefDecl,
    /// `typedef class C;`
    ForwardTypedef,
    /// `constraint name { ... }`, in a class or out of block.
    ConstraintDecl,
    /// `constraint c;` / `extern constraint c;`
    ConstraintPrototype,
    /// `{ ... }` of a constraint, kept as written.
    ConstraintBlock,
    /// `covergroup ... endgroup`.
    CovergroupDecl,
    /// `property ... endproperty`.
    PropertyDecl,
    /// `sequence ... endsequence`.
    SequenceDecl,
    /// `clocking ... endclocking`.
    ClockingDecl,
    /// `import pkg::*;` / `import pkg::name;`
    ImportDecl,
    /// `export pkg::*;` and the like.
    ExportDecl,
    /// `import "DPI-C" ...;`
    DpiImport,
    /// `export "DPI-C" ...;`
    DpiExport,
    /// `parameter` / `localparam` (also a `type` parameter).
    ParamDecl,
    /// A data, net or class-property declaration, or a module
    /// instantiation (they are told apart in a later slice).
    DataDecl,
    /// `assign a = b;`
    ContinuousAssign,
    /// always*, initial, final with its statement.
    ProceduralBlock,
    /// `timeunit` / `timeprecision`.
    TimeunitDecl,
    /// `genvar i;`
    GenvarDecl,
    /// `modport name (...);`
    ModportDecl,
    /// `bind target inst ...;`
    BindDirective,
    /// `let name(args) = expr;`
    LetDecl,
    /// A block kept as written (no longer produced; kept for callers).
    OtherBlock,
    /// Any other item, up to its `;`.
    OtherItem,
    /// A lone `;` at item level.
    EmptyItem,

    // Statements.
    /// The statements of a function or task.
    Body,
    /// `begin ... end`.
    SeqBlock,
    /// `fork ... join` / `join_any` / `join_none`.
    ParBlock,
    /// `if (cond) stmt`, with its `else` as an [`SyntaxKind::ElseClause`] child.
    IfStmt,
    /// `else stmt`.
    ElseClause,
    /// `case`/`casex`/`casez` (with `unique`/`priority`), its items inside.
    CaseStmt,
    /// `for`, `foreach`, `while`, `repeat` and `forever`.
    LoopStmt,
    /// `do stmt while (cond);`
    DoWhileStmt,
    /// `#d stmt`, `@(e) stmt`, `##n stmt`.
    TimedStmt,
    /// `wait (cond) stmt`, `wait fork;`, `wait_order (...)`.
    WaitStmt,
    /// Immediate `assert`/`assume`/`cover` (with an action block).
    AssertStmt,
    /// `randsequence (...) ... endsequence`.
    RandSequenceStmt,
    /// `label:` in front of a statement.
    StmtLabel,
    /// A lone `;` as a statement.
    EmptyStmt,
    /// An expression statement: assignment, call, increment.
    ExprStmt,
    /// `return [expr];`
    ReturnStmt,
    /// `break;` / `continue;`
    JumpStmt,
    /// `disable name;` / `disable fork;`
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
    /// A data type as written: keyword or name, signing, packed dimensions.
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
    /// `( ... )` port list of a module, interface or subroutine header.
    PortList,
    /// One port or subroutine argument.
    Port,
    /// `[edge] expr [iff expr]` in an event control.
    EventExpr,
    /// One constraint (expression, `if`, `foreach`, `unique`, `soft`, ...).
    ConstraintItem,
    /// `expr dist { ... }`
    ExprOrDist,
    /// One `value := weight` or `[range] :/ weight` item of a `dist`.
    DistItem,
    /// `@@(begin f)`
    BlockEvent,
    /// `option.x = v;`
    CoverageOption,
    /// `[label:] coverpoint expr [iff (...)] { ... }`.
    Coverpoint,
    /// `[label:] cross a, b { ... }`.
    Cross,
    /// A `bins`/`illegal_bins`/`ignore_bins` declaration.
    Bins,
    /// A transition list `(a => b), (c => d)` of a bins.
    TransList,
    /// One transition sequence `a => b [*n]` in a list.
    TransItem,
    /// A `binsof(...) intersect {...}` selection in a cross.
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
    /// An item of a clocking block (`input #1 a;`, `default ...`).
    ClockingItem,
    /// `default disable iff expr;`
    DefaultDisable,

    // Slice 4: the rarer items.
    /// `name [= expr]` inside `(* ... *)`.
    AttrSpec,
    /// `nettype T name [with f];`
    NetTypeDecl,
    /// `alias a = b [= c];`
    NetAlias,
    /// `$error(...)`, `$fatal`, `$warning`, `$info` as an item.
    ElabTask,
    /// `specify ... endspecify`
    SpecifyBlock,
    /// `specparam ...;` (module item or in a specify block).
    SpecparamDecl,
    /// `(a => b) = d;` with an optional `if (c)` / `ifnone`.
    PathDecl,
    /// The parenthesized source and destination of a path.
    PathSpec,
    /// `= d` / `= (d1, d2)` of a path.
    PathDelays,
    /// `$setup(...)` and the other timing checks.
    TimingCheck,
    /// One argument of a timing check.
    TimingCheckArg,
    /// `pulsestyle_onevent`, `showcancelled` and friends.
    SpecifyOther,
    /// `primitive ... endprimitive`
    UdpDecl,
    /// A port declaration inside a UDP.
    UdpPortDecl,
    /// `initial q = v;` in a sequential UDP.
    UdpInitial,
    /// `table ... endtable`, kept as written.
    UdpTable,
    /// `config ... endconfig`
    ConfigDecl,
    /// `design`, `default`, `instance`, `cell` rules of a config.
    ConfigRule,
    /// A production of a randsequence: `[type] name [(ports)] : rules ;`
    Production,
    /// One alternative of a production, with its weight.
    RsRule,
    /// `:= weight [code]`
    RsWeight,
    /// `name [(args)]`
    RsProdItem,
    /// `{ statements }`
    RsCodeBlock,
    /// `if (cond) prod else prod` in a randsequence production.
    RsIfElse,
    /// `repeat (n) prod` in a randsequence production.
    RsRepeat,
    /// `case (expr) ... endcase` in a randsequence production.
    RsCase,
    /// One item of a randsequence `case`.
    RsCaseItem,
    /// `rand join [(expr)]`
    RsRandJoin,

    // Expressions.
    /// `a op b`, for every binary operator of Table 11-2.
    BinaryExpr,
    /// Unary prefix operator: `-a`, `!a`, `&a`, `++a`.
    PrefixExpr,
    /// Postfix `a++` / `a--`.
    PostfixExpr,
    /// `c ? a : b`.
    CondExpr,
    /// `a inside { ... }`.
    InsideExpr,
    /// `{ ... }` after `inside`.
    RangeList,
    /// `[lo : hi]` in a range list or case item.
    OpenRange,
    /// `( expr )`.
    ParenExpr,
    /// A number, string, time or unbased-unsized literal.
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
    /// `( ... )` arguments of a call.
    ArgList,
    /// One argument of a call, positional or `.name(expr)`.
    Arg,
    /// `with (...)` / `with { ... }` after a method call.
    WithClause,
    /// `{a, b}` / `{n{a}}` concatenation, and streaming `{<< {...}}`.
    ConcatExpr,
    /// `'{ ... }`
    AssignPattern,
    /// `T'(x)`, `8'(x)`, `T'{...}`
    CastExpr,
    /// `new`, `new(args)`, `new[n]`, and the shallow copy `new src`.
    NewExpr,
    /// `tagged Member [expr]`.
    TaggedExpr,
    /// `x matches pattern`, `a &&& b` in a condition.
    MatchExpr,
    /// A pattern after `matches` (`.x`, `.*`, `tagged T ...`, `'{...}`).
    Pattern,
    /// `#1 x` inside an assignment.
    TimedExpr,
    /// A type standing where an expression may: `$bits(int)`.
    TypeExpr,
}
