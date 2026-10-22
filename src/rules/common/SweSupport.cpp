#include "uvmcheck/rules/SweSupport.h"

#include <cctype>
#include <unordered_set>

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/ast/symbols/InstanceSymbols.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxNode.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::rules::swe {

using namespace slang::ast;

const char* roleName(Role role) {
    switch (role) {
        case Role::None: return "class";
        case Role::Object: return "object";
        case Role::Config: return "config";
        case Role::Item: return "sequence item";
        case Role::Sequence: return "sequence";
        case Role::Component: return "component";
        case Role::Driver: return "driver";
        case Role::Monitor: return "monitor";
        case Role::Sequencer: return "sequencer";
        case Role::Agent: return "agent";
        case Role::Scoreboard: return "scoreboard";
        case Role::Subscriber: return "subscriber";
        case Role::Env: return "env";
        case Role::Test: return "test";
        case Role::RegModel: return "register model";
        case Role::Catcher: return "report catcher";
        case Role::Callback: return "callback";
    }
    return "class";
}

bool isComponentRole(Role role) {
    switch (role) {
        case Role::Component: case Role::Driver: case Role::Monitor: case Role::Sequencer:
        case Role::Agent: case Role::Scoreboard: case Role::Subscriber: case Role::Env:
        case Role::Test:
            return true;
        default:
            return false;
    }
}

static std::string_view className(const ClassType& cls) {
    return cls.genericClass ? cls.genericClass->name : cls.name;
}

std::string_view uvmBaseName(const ClassType& cls) {
    const Type* curr = cls.getBaseClass();
    int guard = 0;
    while (curr && curr->isClass() && guard++ < 64) {
        const auto& ct = curr->getCanonicalType().as<ClassType>();
        std::string_view name = className(ct);
        if (name.starts_with("uvm_")) return name;
        curr = ct.getBaseClass();
    }
    return {};
}

static bool nameSaysConfig(std::string_view name) {
    std::string lower;
    for (char c : name) lower += static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    return lower.find("cfg") != std::string::npos || lower.find("config") != std::string::npos;
}

Role classRole(const ClassType& cls) {
    // Walk up to the first uvm_* base; its name decides the role.
    const Type* curr = cls.getBaseClass();
    int guard = 0;
    while (curr && curr->isClass() && guard++ < 64) {
        const auto& ct = curr->getCanonicalType().as<ClassType>();
        std::string_view n = className(ct);
        if (n.starts_with("uvm_")) {
            if (n == "uvm_driver" || n == "uvm_push_driver") return Role::Driver;
            if (n == "uvm_monitor") return Role::Monitor;
            if (n.starts_with("uvm_sequencer") || n == "uvm_push_sequencer") return Role::Sequencer;
            if (n == "uvm_agent") return Role::Agent;
            if (n == "uvm_scoreboard") return Role::Scoreboard;
            if (n == "uvm_subscriber") return Role::Subscriber;
            if (n == "uvm_env") return Role::Env;
            if (n == "uvm_test") return Role::Test;
            if (n.starts_with("uvm_reg") || n == "uvm_mem" || n == "uvm_vreg") return Role::RegModel;
            if (n == "uvm_report_catcher") return Role::Catcher;
            if (n.starts_with("uvm_callback")) return Role::Callback;
            if (n == "uvm_sequence" || n == "uvm_sequence_base" || n == "uvm_reg_sequence")
                return Role::Sequence;
            if (n == "uvm_sequence_item" || n == "uvm_transaction") {
                // uvm_sequence_base derives from uvm_sequence_item; seen first above.
                return Role::Item;
            }
            if (n == "uvm_component" || n.starts_with("uvm_tlm") || n == "uvm_root")
                return Role::Component;
            if (n == "uvm_object" || n == "uvm_void")
                return nameSaysConfig(className(cls)) ? Role::Config : Role::Object;
            // Some other uvm_* class: keep walking to a known one.
        }
        curr = ct.getBaseClass();
    }
    return Role::None;
}

static const slang::SourceManager* sourceManagerFor(const Symbol& sym) {
    const Scope* scope = sym.getParentScope();
    if (!scope) {
        if (sym.isScope()) scope = &sym.as<Scope>();
        else return nullptr;
    }
    return scope->getCompilation().getSourceManager();
}

bool isLibrarySymbol(const Symbol& sym) {
    for (const Symbol* s = &sym; s; ) {
        if (s->kind == SymbolKind::Package && s->name == "uvm_pkg") return true;
        if ((s->kind == SymbolKind::ClassType || s->kind == SymbolKind::GenericClassDef) &&
            s->name.starts_with("uvm_")) {
            return true;
        }
        const Scope* parent = s->getParentScope();
        s = parent ? &parent->asSymbol() : nullptr;
    }
    if (sym.location.valid()) {
        if (const auto* sm = sourceManagerFor(sym)) {
            auto loc = sm->getFullyOriginalLoc(sym.location);
            std::string_view file = sm->getFileName(loc);
            if (file.find("uvm_mock.svh") != std::string_view::npos) return true;
            if (file.find("uvm_pkg.sv") != std::string_view::npos) return true;
        }
    }
    return false;
}

static void collectClasses(const Scope& scope, std::vector<const ClassType*>& out,
                           std::unordered_set<const Symbol*>& seen) {
    for (const auto& member : scope.members()) {
        if (member.kind == SymbolKind::ClassType) {
            const auto& ct = member.as<ClassType>();
            if (seen.insert(&ct).second) out.push_back(&ct);
            collectClasses(ct, out, seen);
        } else if (member.kind == SymbolKind::GenericClassDef) {
            const auto& gen = member.as<GenericClassDefSymbol>();
            if (const Type* spec = gen.getDefaultSpecialization(*gen.getParentScope())) {
                if (spec->isClass()) {
                    const auto& ct = spec->getCanonicalType().as<ClassType>();
                    if (seen.insert(&ct).second) out.push_back(&ct);
                }
            }
        } else if (member.kind == SymbolKind::Package) {
            collectClasses(member.as<PackageSymbol>(), out, seen);
        } else if (member.kind == SymbolKind::CompilationUnit) {
            collectClasses(member.as<CompilationUnitSymbol>(), out, seen);
        }
    }
}

std::vector<const ClassType*> userClasses(const uvm::UvmModel& model) {
    std::vector<const ClassType*> out;
    std::unordered_set<const Symbol*> seen;
    for (const auto* cls : model.getAllClasses()) {
        if (seen.insert(cls).second) out.push_back(cls);
    }
    if (!model.getAllClasses().empty()) {
        // UvmModel skips parameterized classes; add their default specializations.
        const auto& root = model.getAllClasses().front()->getCompilation().getRoot();
        collectClasses(root, out, seen);
    }
    std::vector<const ClassType*> filtered;
    for (const auto* cls : out) {
        if (!isLibrarySymbol(*cls)) filtered.push_back(cls);
    }
    return filtered;
}

std::vector<const SubroutineSymbol*> classMethods(const ClassType& cls) {
    std::vector<const SubroutineSymbol*> out;
    std::unordered_set<const SubroutineSymbol*> seen;
    auto add = [&](const SubroutineSymbol* sub) {
        if (!sub || sub->flags.has(MethodFlags::BuiltIn) || sub->flags.has(MethodFlags::Pure) ||
            sub->flags.has(MethodFlags::DPIImport)) {
            return;
        }
        if (seen.insert(sub).second) out.push_back(sub);
    };
    for (const auto& member : cls.members()) {
        if (member.kind == SymbolKind::Subroutine) {
            add(&member.as<SubroutineSymbol>());
        } else if (member.kind == SymbolKind::MethodPrototype) {
            add(member.as<MethodPrototypeSymbol>().getSubroutine());
        }
    }
    return out;
}

static void collectRoutines(const Scope& scope, std::vector<const SubroutineSymbol*>& out,
                            std::unordered_set<const Symbol*>& seen, int depth) {
    if (depth > 64) return;
    for (const auto& member : scope.members()) {
        switch (member.kind) {
            case SymbolKind::Subroutine: {
                const auto& sub = member.as<SubroutineSymbol>();
                if (!sub.flags.has(MethodFlags::BuiltIn) && !sub.flags.has(MethodFlags::DPIImport) &&
                    !isLibrarySymbol(sub) && seen.insert(&sub).second) {
                    out.push_back(&sub);
                }
                break;
            }
            case SymbolKind::Package:
                collectRoutines(member.as<PackageSymbol>(), out, seen, depth + 1);
                break;
            case SymbolKind::CompilationUnit:
                collectRoutines(member.as<CompilationUnitSymbol>(), out, seen, depth + 1);
                break;
            case SymbolKind::Instance: {
                const auto& inst = member.as<InstanceSymbol>();
                if (seen.insert(&inst.body).second)
                    collectRoutines(inst.body, out, seen, depth + 1);
                break;
            }
            default:
                break;
        }
    }
}

std::vector<const SubroutineSymbol*> freeRoutines(Compilation& comp) {
    std::vector<const SubroutineSymbol*> out;
    std::unordered_set<const Symbol*> seen;
    collectRoutines(comp.getRoot(), out, seen, 0);
    for (const auto* top : comp.getRoot().topInstances) {
        if (seen.insert(&top->body).second) collectRoutines(top->body, out, seen, 1);
    }
    return out;
}

std::vector<const SubroutineSymbol*> allUserRoutines(Compilation& comp, const uvm::UvmModel& model) {
    std::vector<const SubroutineSymbol*> out;
    std::unordered_set<const SubroutineSymbol*> seen;
    for (const auto* cls : userClasses(model)) {
        for (const auto* m : classMethods(*cls)) {
            if (seen.insert(m).second) out.push_back(m);
        }
    }
    for (const auto* r : freeRoutines(comp)) {
        if (seen.insert(r).second) out.push_back(r);
    }
    return out;
}

static void appendTokens(const slang::syntax::SyntaxNode& node, std::string& out) {
    size_t n = node.getChildCount();
    for (size_t i = 0; i < n; i++) {
        if (const auto* child = node.childNode(i)) {
            appendTokens(*child, out);
            continue;
        }
        slang::parsing::Token tok = node.childToken(i);
        if (!tok || tok.isMissing() || tok.rawText().empty()) continue;
        // Any trivia (whitespace, newlines or comments) becomes one space; comments are dropped.
        if (!out.empty() && !tok.trivia().empty()) out += ' ';
        out += tok.rawText();
    }
}

std::string normalizedText(const slang::syntax::SyntaxNode* node) {
    if (!node) return {};
    std::string out;
    appendTokens(*node, out);
    return out;
}

std::string exprText(const Expression& expr) { return normalizedText(expr.syntax); }
std::string stmtText(const Statement& stmt) { return normalizedText(stmt.syntax); }

slang::SourceLocation locOf(const Statement& stmt) { return stmt.sourceRange.start(); }
slang::SourceLocation locOf(const Expression& expr) { return expr.sourceRange.start(); }

const Expression& stripConversions(const Expression& expr) {
    const Expression* e = &expr;
    while (e->kind == ExpressionKind::Conversion) e = &e->as<ConversionExpression>().operand();
    return *e;
}

const Symbol* referencedSymbol(const Expression& expr) {
    const Expression& e = stripConversions(expr);
    switch (e.kind) {
        case ExpressionKind::NamedValue:
        case ExpressionKind::HierarchicalValue:
            return &e.as<ValueExpressionBase>().symbol;
        case ExpressionKind::MemberAccess:
            return &e.as<MemberAccessExpression>().member;
        default:
            return nullptr;
    }
}

const ClassType* asClass(const Type& type) {
    const Type& canon = type.getCanonicalType();
    if (!canon.isClass()) return nullptr;
    return &canon.as<ClassType>();
}

ReportLevel reportLevel(const CallExpression& call) {
    std::string_view n = call.getSubroutineName();
    if (call.isSystemCall()) {
        if (n == "$fatal") return ReportLevel::Fatal;
        if (n == "$error") return ReportLevel::Error;
        if (n == "$warning") return ReportLevel::Warning;
        if (n == "$info") return ReportLevel::Info;
        return ReportLevel::None;
    }
    if (n == "uvm_report_fatal") return ReportLevel::Fatal;
    if (n == "uvm_report_error") return ReportLevel::Error;
    if (n == "uvm_report_warning") return ReportLevel::Warning;
    if (n == "uvm_report_info") return ReportLevel::Info;
    return ReportLevel::None;
}

namespace {
struct CallCollector : public ASTVisitor<CallCollector, VisitFlags::AllGood> {
    const std::function<void(const CallExpression&)>* fn = nullptr;
    void handle(const CallExpression& call) {
        (*fn)(call);
        visitDefault(call);
    }
};

struct StatementCounter : public ASTVisitor<StatementCounter, VisitFlags::Statements> {
    int count = 0;
    template<typename T>
    void handle(const T& node) {
        if constexpr (std::is_base_of_v<Statement, T>) {
            // A `uvm_* macro counts as the one report call it stands for.
            if (isReportScaffolding(node)) return;
            if constexpr (std::is_same_v<T, ConditionalStatement>) {
                if (isReportGuard(node)) {
                    node.ifTrue.visit(*this);
                    return;
                }
            }
            if (node.kind != StatementKind::Block && node.kind != StatementKind::List &&
                node.kind != StatementKind::Empty && node.kind != StatementKind::Invalid) {
                count++;
            }
        }
        visitDefault(node);
    }
};
} // namespace

void forEachCall(const Statement& stmt, const std::function<void(const CallExpression&)>& fn) {
    CallCollector v;
    v.fn = &fn;
    stmt.visit(v);
}

ReportLevel strongestReport(const Statement& stmt) {
    ReportLevel best = ReportLevel::None;
    forEachCall(stmt, [&](const CallExpression& call) {
        ReportLevel l = reportLevel(call);
        if (static_cast<int>(l) > static_cast<int>(best)) best = l;
    });
    return best;
}

static bool isStopCall(const Expression& expr) {
    const Expression& e = stripConversions(expr);
    if (e.kind != ExpressionKind::Call) return false;
    const auto& call = e.as<CallExpression>();
    std::string_view n = call.getSubroutineName();
    if (call.isSystemCall()) return n == "$fatal" || n == "$finish" || n == "$exit";
    return n == "uvm_report_fatal";
}

bool alwaysExits(const Statement& stmt) {
    switch (stmt.kind) {
        case StatementKind::Return:
        case StatementKind::Break:
        case StatementKind::Continue:
            return true;
        case StatementKind::ExpressionStatement:
            return isStopCall(stmt.as<ExpressionStatement>().expr);
        case StatementKind::Block: {
            const auto& b = stmt.as<BlockStatement>();
            if (b.blockKind == StatementBlockKind::JoinNone) return false;
            return alwaysExits(b.body);
        }
        case StatementKind::List:
            for (const auto* s : stmt.as<StatementList>().list) {
                if (s && alwaysExits(*s)) return true;
            }
            return false;
        case StatementKind::Conditional: {
            const auto& c = stmt.as<ConditionalStatement>();
            // `uvm_fatal expands to a report guard around uvm_report_fatal(...).
            if (isReportGuard(c) && alwaysExits(c.ifTrue)) return true;
            return c.ifFalse && alwaysExits(c.ifTrue) && alwaysExits(*c.ifFalse);
        }
        case StatementKind::Case: {
            const auto& cs = stmt.as<CaseStatement>();
            if (!cs.defaultCase || !alwaysExits(*cs.defaultCase)) return false;
            for (const auto& item : cs.items) {
                if (!alwaysExits(*item.stmt)) return false;
            }
            return true;
        }
        case StatementKind::Timed:
            return alwaysExits(stmt.as<TimedStatement>().stmt);
        default:
            return false;
    }
}

int countStatements(const Statement& stmt) {
    StatementCounter c;
    stmt.visit(c);
    return c.count;
}

int64_t ruleOption(std::string_view /*ruleId*/, std::string_view /*key*/, int64_t defaultValue) {
    return defaultValue;
}

bool derivesFrom(const ClassType& cls, const ClassType& base) {
    const Type* curr = cls.getBaseClass();
    int guard = 0;
    while (curr && curr->isClass() && guard++ < 64) {
        const auto& ct = curr->getCanonicalType().as<ClassType>();
        if (&ct == &base) return true;
        if (base.genericClass && ct.genericClass == base.genericClass) return true;
        curr = ct.getBaseClass();
    }
    return false;
}

std::string receiverText(const CallExpression& call) {
    if (call.thisClass()) return exprText(*call.thisClass());
    return {};
}

bool isConstantExpr(const Expression& expr) {
    const Expression& e = stripConversions(expr);
    if (e.getConstant()) return true;
    switch (e.kind) {
        case ExpressionKind::IntegerLiteral:
        case ExpressionKind::RealLiteral:
        case ExpressionKind::TimeLiteral:
        case ExpressionKind::UnbasedUnsizedIntegerLiteral:
            return true;
        case ExpressionKind::NamedValue: {
            const auto& sym = e.as<NamedValueExpression>().symbol;
            return sym.kind == SymbolKind::Parameter || sym.kind == SymbolKind::Specparam;
        }
        case ExpressionKind::BinaryOp: {
            const auto& b = e.as<BinaryExpression>();
            return isConstantExpr(b.left()) && isConstantExpr(b.right());
        }
        default:
            return false;
    }
}

bool isFixedDelay(const TimingControl& timing) {
    if (timing.kind != TimingControlKind::Delay) return false;
    return isConstantExpr(timing.as<DelayControl>().expr);
}

static std::string stripCommentsAndSpace(std::string_view src) {
    std::string out;
    bool space = false;
    for (size_t i = 0; i < src.size(); i++) {
        char c = src[i];
        if (c == '"') { // copy string literals verbatim
            if (space && !out.empty()) out += ' ';
            space = false;
            out += c;
            for (i++; i < src.size(); i++) {
                out += src[i];
                if (src[i] == '\\' && i + 1 < src.size()) { out += src[++i]; continue; }
                if (src[i] == '"') break;
            }
            continue;
        }
        if (c == '/' && i + 1 < src.size() && src[i + 1] == '/') {
            while (i < src.size() && src[i] != '\n') i++;
            space = true;
            continue;
        }
        if (c == '/' && i + 1 < src.size() && src[i + 1] == '*') {
            i += 2;
            while (i + 1 < src.size() && !(src[i] == '*' && src[i + 1] == '/')) i++;
            i++;
            space = true;
            continue;
        }
        if (std::isspace(static_cast<unsigned char>(c))) { space = true; continue; }
        if (space && !out.empty()) out += ' ';
        space = false;
        out += c;
    }
    return out;
}

std::string originalText(const Compilation& comp, const Statement& stmt) {
    const auto* sm = comp.getSourceManager();
    if (!sm) return stmtText(stmt);
    // Map macro-expanded endpoints back to the macro call as the user wrote it.
    auto start = stmt.sourceRange.start();
    auto end = stmt.sourceRange.end();
    for (int guard = 0; guard < 32 && sm->isMacroLoc(start); guard++) start = sm->getExpansionRange(start).start();
    for (int guard = 0; guard < 32 && sm->isMacroLoc(end); guard++) end = sm->getExpansionRange(end).end();
    slang::SourceRange range(start, end);
    if (!range.start().valid() || range.start().buffer() != range.end().buffer() ||
        range.end().offset() < range.start().offset()) {
        return stmtText(stmt);
    }
    std::string_view text = sm->getSourceText(range.start().buffer());
    if (range.end().offset() > text.size()) return stmtText(stmt);
    return stripCommentsAndSpace(
        text.substr(range.start().offset(), range.end().offset() - range.start().offset()));
}

size_t lineOf(const Compilation& comp, slang::SourceLocation loc) {
    const auto* sm = comp.getSourceManager();
    if (!sm || !loc.valid()) return 0;
    return sm->getLineNumber(sm->getFullyExpandedLoc(loc));
}

} // namespace uvmcheck::rules::swe

namespace uvmcheck::rules::swe {

int positionalArgCount(const CallExpression& call) {
    const auto* syntax = call.syntax;
    if (!syntax || syntax->kind != slang::syntax::SyntaxKind::InvocationExpression) return -1;
    const auto* list = syntax->as<slang::syntax::InvocationExpressionSyntax>().arguments;
    if (!list) return 0;
    int n = 0;
    for (const auto* p : list->parameters) {
        if (p->kind != slang::syntax::SyntaxKind::OrderedArgument) return -1;
        n++;
    }
    return n;
}

const Symbol* rootSymbol(const Expression& expr) {
    const Expression* e = &stripConversions(expr);
    for (int guard = 0; guard < 64; guard++) {
        switch (e->kind) {
            case ExpressionKind::NamedValue:
            case ExpressionKind::HierarchicalValue:
                return &e->as<ValueExpressionBase>().symbol;
            case ExpressionKind::ElementSelect:
                e = &stripConversions(e->as<ElementSelectExpression>().value());
                break;
            case ExpressionKind::RangeSelect:
                e = &stripConversions(e->as<RangeSelectExpression>().value());
                break;
            case ExpressionKind::MemberAccess:
                e = &stripConversions(e->as<MemberAccessExpression>().value());
                break;
            default:
                return nullptr;
        }
    }
    return nullptr;
}

} // namespace uvmcheck::rules::swe

namespace uvmcheck::rules::swe {

namespace {

bool isMutatingSystemCall(std::string_view n, bool containerMethods) {
    if (n == "$cast" || n == "$sscanf" || n == "$fscanf" || n == "$fgets" || n == "$fread" ||
        n == "$value$plusargs" || n == "randomize" || n == "srandom")
        return true;
    return containerMethods && (n == "push_back" || n == "push_front" || n == "pop_back" || n == "pop_front" ||
           n == "insert" || n == "delete" || n == "sort" || n == "rsort" || n == "reverse" ||
           n == "shuffle");
}

// Collects every variable written in a body: assigned, incremented, passed to an output or
// non-const ref argument, or changed by an array method.
class WriteCollector : public ASTVisitor<WriteCollector, VisitFlags::AllGood> {
public:
    std::unordered_set<const Symbol*> written;
    bool containerMethods = true;

    void handle(const AssignmentExpression& a) {
        if (const auto* s = swe::rootSymbol(a.left())) written.insert(s);
        visitDefault(a);
    }

    void handle(const UnaryExpression& u) {
        switch (u.op) {
            case UnaryOperator::Preincrement: case UnaryOperator::Predecrement:
            case UnaryOperator::Postincrement: case UnaryOperator::Postdecrement:
                if (const auto* s = swe::rootSymbol(u.operand())) written.insert(s);
                break;
            default: break;
        }
        visitDefault(u);
    }

    void handle(const CallExpression& call) {
        auto actuals = call.arguments();
        if (call.isSystemCall()) {
            if (isMutatingSystemCall(call.getSubroutineName(), containerMethods)) {
                for (const auto* a : actuals) {
                    if (const auto* s = swe::rootSymbol(*a)) written.insert(s);
                }
            }
        } else if (const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine); sub && *sub) {
            auto formals = (*sub)->getArguments();
            for (size_t i = 0; i < formals.size() && i < actuals.size(); i++) {
                if (formals[i]->direction == ArgumentDirection::In) continue;
                if (formals[i]->direction == ArgumentDirection::Ref && formals[i]->flags.has(VariableFlags::Const))
                    continue;
                if (const auto* s = swe::rootSymbol(*actuals[i])) written.insert(s);
            }
            if (call.getSubroutineName() == "randomize" && call.thisClass()) {
                if (const auto* s = swe::rootSymbol(*call.thisClass())) written.insert(s);
            }
        }
        visitDefault(call);
    }
};


class ReadCollector : public ASTVisitor<ReadCollector, VisitFlags::AllGood> {
public:
    std::unordered_set<const Symbol*> used;
    void handle(const NamedValueExpression& e) {
        used.insert(&e.symbol);
        visitDefault(e);
    }
    void handle(const MemberAccessExpression& e) {
        used.insert(&e.member);
        visitDefault(e);
    }
};

} // namespace

std::unordered_set<const Symbol*> writtenSymbols(const Statement& stmt, bool containerMethods) {
    WriteCollector w;
    w.containerMethods = containerMethods;
    stmt.visit(w);
    return std::move(w.written);
}

std::unordered_set<const Symbol*> referencedSymbols(const Expression& expr) {
    ReadCollector r;
    expr.visit(r);
    return std::move(r.used);
}

bool isSemaphoreGet(const CallExpression& call) {
    std::string_view n = call.getSubroutineName();
    if (n != "get" && n != "try_get") return false;
    const Expression* recv = call.thisClass();
    if (!recv || !recv->type) return false;
    const ClassType* c = asClass(*recv->type);
    return c && c->name == "semaphore";
}

} // namespace uvmcheck::rules::swe

namespace uvmcheck::rules::swe {

static bool isReportQuery(std::string_view n) {
    return n == "uvm_report_enabled" || n == "get_report_verbosity_level" || n == "get_report_action" ||
           n == "uvm_get_report_object";
}

bool isReportGuard(const ConditionalStatement& c) {
    if (c.conditions.size() != 1 || c.conditions[0].pattern) return false;
    struct Calls : public ASTVisitor<Calls, VisitFlags::AllGood> {
        bool report = false, other = false;
        void handle(const CallExpression& call) {
            (isReportQuery(call.getSubroutineName()) ? report : other) = true;
            visitDefault(call);
        }
    } calls;
    c.conditions[0].expr->visit(calls);
    return calls.report && !calls.other;
}

static bool isReportObjectLocal(std::string_view name) {
    return name.starts_with("_local_report_object_");
}

bool isReportScaffolding(const Statement& s) {
    if (s.kind == StatementKind::VariableDeclaration)
        return isReportObjectLocal(s.as<VariableDeclStatement>().symbol.name);
    if (s.kind == StatementKind::ExpressionStatement) {
        const Expression& e = s.as<ExpressionStatement>().expr;
        if (e.kind != ExpressionKind::Assignment) return false;
        const Symbol* lhs = referencedSymbol(e.as<AssignmentExpression>().left());
        return lhs && isReportObjectLocal(lhs->name);
    }
    return false;
}

} // namespace uvmcheck::rules::swe
