#include "uvmcheck/rules/CodeMetrics.h"

#include <algorithm>
#include <map>
#include <mutex>
#include <numeric>
#include <set>

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/MemberSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::metrics {

using namespace slang::ast;

namespace {

// `uvm_* macros expand to a report guard; it is not the author's branching.
bool isReportGuard(const ConditionalStatement& c) { return swe::isReportGuard(c); }

int logicalOps(const Expression& expr) {
    struct Counter : public ASTVisitor<Counter, VisitFlags::Expressions> {
        int n = 0;
        void handle(const BinaryExpression& b) {
            if (b.op == BinaryOperator::LogicalAnd || b.op == BinaryOperator::LogicalOr) n++;
            visitDefault(b);
        }
    } c;
    expr.visit(c);
    return c.n;
}

// Sonar: one increment per sequence of like logical operators (a && b && c = 1, a && b || c = 2).
int logicalSequences(const Expression& expr, std::optional<BinaryOperator> parent = std::nullopt) {
    const Expression& e = swe::stripConversions(expr);
    if (e.kind == ExpressionKind::BinaryOp) {
        const auto& b = e.as<BinaryExpression>();
        if (b.op == BinaryOperator::LogicalAnd || b.op == BinaryOperator::LogicalOr) {
            int self = (!parent || *parent != b.op) ? 1 : 0;
            return self + logicalSequences(b.left(), b.op) + logicalSequences(b.right(), b.op);
        }
        return logicalSequences(b.left()) + logicalSequences(b.right());
    }
    if (e.kind == ExpressionKind::UnaryOp) return logicalSequences(e.as<UnaryExpression>().operand());
    return 0;
}

class RoutineWalker {
public:
    RoutineMetrics m;

    void condition(const Expression& e) {
        int ops = logicalOps(e);
        if (ops > m.maxBoolOps) {
            m.maxBoolOps = ops;
            m.mostComplexCondition = swe::locOf(e);
        }
        m.cognitive += logicalSequences(e);
        m.cyclomatic += ops;
    }

    void structure(const Statement& s, int nesting) {
        m.cognitive += 1 + nesting;
        if (nesting + 1 > m.maxNesting) {
            m.maxNesting = nesting + 1;
            m.deepest = swe::locOf(s);
        }
    }

    void expressions(const Statement& s, int nesting) {
        // Ternaries inside plain statements.
        struct Ternaries : public ASTVisitor<Ternaries, VisitFlags::Expressions> {
            RoutineWalker* w; int nesting;
            void handle(const ConditionalExpression& c) {
                w->m.cognitive += 1 + nesting;
                w->m.cyclomatic += 1;
                for (const auto& cond : c.conditions) w->condition(*cond.expr);
                visitDefault(c);
            }
        } t;
        t.w = this;
        t.nesting = nesting;
        s.visit(t);
    }

    void walk(const Statement& s, int nesting) {
        switch (s.kind) {
            case StatementKind::List:
                for (const auto* c : s.as<StatementList>().list) if (c) walk(*c, nesting);
                return;
            case StatementKind::Block: {
                const auto& b = s.as<BlockStatement>();
                if (b.blockKind != StatementBlockKind::Sequential) {
                    structure(s, nesting);
                    walk(b.body, nesting + 1);
                } else {
                    walk(b.body, nesting);
                }
                return;
            }
            case StatementKind::Conditional: {
                const auto& c = s.as<ConditionalStatement>();
                if (isReportGuard(c)) {
                    walk(c.ifTrue, nesting);
                    return;
                }
                structure(s, nesting);
                m.cyclomatic++;
                for (const auto& cond : c.conditions) condition(*cond.expr);
                walk(c.ifTrue, nesting + 1);
                const Statement* els = c.ifFalse;
                while (els) {
                    m.cognitive += 1; // else / else if: no nesting increment
                    if (els->kind == StatementKind::Conditional &&
                        !isReportGuard(els->as<ConditionalStatement>())) {
                        const auto& ei = els->as<ConditionalStatement>();
                        m.cyclomatic++;
                        for (const auto& cond : ei.conditions) condition(*cond.expr);
                        walk(ei.ifTrue, nesting + 1);
                        els = ei.ifFalse;
                    } else {
                        walk(*els, nesting + 1);
                        els = nullptr;
                    }
                }
                return;
            }
            case StatementKind::Case: {
                const auto& cs = s.as<CaseStatement>();
                structure(s, nesting);
                m.cyclomatic += static_cast<int>(cs.items.size());
                for (const auto& item : cs.items) walk(*item.stmt, nesting + 1);
                if (cs.defaultCase) walk(*cs.defaultCase, nesting + 1);
                return;
            }
            case StatementKind::ForLoop: {
                const auto& l = s.as<ForLoopStatement>();
                structure(s, nesting);
                m.cyclomatic++;
                if (l.stopExpr) condition(*l.stopExpr);
                walk(l.body, nesting + 1);
                return;
            }
            case StatementKind::WhileLoop: {
                const auto& l = s.as<WhileLoopStatement>();
                structure(s, nesting);
                m.cyclomatic++;
                condition(l.cond);
                walk(l.body, nesting + 1);
                return;
            }
            case StatementKind::DoWhileLoop: {
                const auto& l = s.as<DoWhileLoopStatement>();
                structure(s, nesting);
                m.cyclomatic++;
                condition(l.cond);
                walk(l.body, nesting + 1);
                return;
            }
            case StatementKind::ForeachLoop:
                structure(s, nesting);
                m.cyclomatic++;
                walk(s.as<ForeachLoopStatement>().body, nesting + 1);
                return;
            case StatementKind::RepeatLoop:
                structure(s, nesting);
                m.cyclomatic++;
                walk(s.as<RepeatLoopStatement>().body, nesting + 1);
                return;
            case StatementKind::ForeverLoop:
                structure(s, nesting);
                m.cyclomatic++;
                walk(s.as<ForeverLoopStatement>().body, nesting + 1);
                return;
            case StatementKind::RandCase: {
                structure(s, nesting);
                const auto& rc = s.as<RandCaseStatement>();
                m.cyclomatic += static_cast<int>(rc.items.size());
                for (const auto& item : rc.items) walk(*item.stmt, nesting + 1);
                return;
            }
            case StatementKind::Timed:
                walk(s.as<TimedStatement>().stmt, nesting);
                return;
            case StatementKind::Wait:
                condition(s.as<WaitStatement>().cond);
                walk(s.as<WaitStatement>().stmt, nesting);
                return;
            case StatementKind::ImmediateAssertion: {
                const auto& a = s.as<ImmediateAssertionStatement>();
                condition(a.cond);
                if (a.ifTrue) walk(*a.ifTrue, nesting);
                if (a.ifFalse) walk(*a.ifFalse, nesting);
                return;
            }
            default:
                expressions(s, nesting);
                return;
        }
    }
};

const ClassType* parentClass(const Symbol& sym) {
    const Scope* scope = sym.getParentScope();
    if (!scope) return nullptr;
    const Symbol& owner = scope->asSymbol();
    return owner.kind == SymbolKind::ClassType ? &owner.as<ClassType>() : nullptr;
}

std::string_view baseName(const ClassType& c) { return c.genericClass ? c.genericClass->name : c.name; }

// Unwraps arrays and queues to the element class, if any.
const ClassType* elementClass(const Type& type) {
    const Type* t = &type.getCanonicalType();
    int guard = 0;
    while (t && guard++ < 8) {
        if (t->isClass()) return &t->as<ClassType>();
        const Type* el = t->getArrayElementType();
        if (!el) return nullptr;
        t = &el->getCanonicalType();
    }
    return nullptr;
}

struct UsageCollector : public ASTVisitor<UsageCollector, VisitFlags::AllGood> {
    const ClassType* self = nullptr;
    std::set<const Symbol*> ownFields;
    std::set<const SubroutineSymbol*> ownCalls;
    std::set<std::string> foreignData;
    std::set<std::string> types;
    std::map<std::string, const ClassType*> typeSyms;

    bool isOwn(const ClassType* c) const {
        return c && (c == self || (self->genericClass && c->genericClass == self->genericClass));
    }

    void noteType(const Type* t) {
        if (!t) return;
        if (const auto* c = elementClass(*t)) {
            if (!swe::isLibrarySymbol(*c) && !isOwn(c)) {
                types.insert(std::string(baseName(*c)));
                typeSyms.emplace(std::string(baseName(*c)), c);
            }
        }
    }

    void handle(const NamedValueExpression& e) {
        if (e.symbol.kind == SymbolKind::ClassProperty && isOwn(parentClass(e.symbol)))
            ownFields.insert(&e.symbol);
        noteType(e.type);
        visitDefault(e);
    }

    void handle(const MemberAccessExpression& e) {
        if (e.member.kind == SymbolKind::ClassProperty) {
            const ClassType* owner = parentClass(e.member);
            if (isOwn(owner)) {
                ownFields.insert(&e.member);
            } else if (owner && !swe::isLibrarySymbol(*owner) && !swe::derivesFrom(*self, *owner)) {
                foreignData.insert(std::string(baseName(*owner)) + "::" + std::string(e.member.name));
            }
        }
        noteType(e.type);
        visitDefault(e);
    }

    void handle(const CallExpression& call) {
        if (!call.isSystemCall()) {
            if (const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine)) {
                if (*sub && isOwn(parentClass(**sub))) ownCalls.insert(*sub);
            }
        }
        noteType(call.type);
        visitDefault(call);
    }

    void handle(const VariableSymbol& v) {
        noteType(&v.getType());
        visitDefault(v);
    }
};

struct UnionFind {
    std::vector<int> p;
    explicit UnionFind(size_t n) : p(n) { std::iota(p.begin(), p.end(), 0); }
    int find(int x) { return p[x] == x ? x : p[x] = find(p[x]); }
    void join(int a, int b) { p[find(a)] = find(b); }
};

ClassMetrics computeClass(const ClassType& cls) {
    ClassMetrics cm;
    cm.cls = &cls;

    // Inheritance depth above the first library or root class.
    const Type* base = cls.getBaseClass();
    int guard = 0;
    while (base && guard++ < 64) {
        const auto* bc = swe::asClass(*base);
        if (!bc || swe::isLibrarySymbol(*bc)) break;
        cm.ancestry.push_back(std::string(baseName(*bc)));
        base = bc->getBaseClass();
    }
    cm.dit = static_cast<int>(cm.ancestry.size());

    UsageCollector typeRefs;
    typeRefs.self = &cls;
    for (const auto& member : cls.members()) {
        if (member.kind == SymbolKind::ClassProperty) {
            const auto& prop = member.as<ClassPropertySymbol>();
            if (prop.flags.has(VariableFlags::CompilerGenerated)) continue;
            cm.fields++;
            typeRefs.noteType(&prop.getType());
        }
    }

    auto methods = swe::classMethods(cls);
    cm.methods = static_cast<int>(methods.size());

    struct MethodUse {
        const SubroutineSymbol* sub;
        std::set<const Symbol*> fields;
        std::set<const SubroutineSymbol*> calls;
    };
    std::vector<MethodUse> uses;
    std::set<std::string> foreign;
    for (const auto* m : methods) {
        cm.wmc += computeRoutineMetrics(*m).cyclomatic;
        UsageCollector u;
        u.self = &cls;
        for (const auto* arg : m->getArguments()) u.noteType(&arg->getType());
        u.noteType(&m->getReturnType());
        m->getBody().visit(u);
        foreign.insert(u.foreignData.begin(), u.foreignData.end());
        typeRefs.types.insert(u.types.begin(), u.types.end());
        typeRefs.typeSyms.insert(u.typeSyms.begin(), u.typeSyms.end());
        if (!m->flags.has(MethodFlags::Constructor) && (!u.ownFields.empty() || !u.ownCalls.empty()))
            uses.push_back({m, u.ownFields, u.ownCalls});
    }
    for (const auto& a : cm.ancestry) typeRefs.types.erase(a);
    cm.atfd = static_cast<int>(foreign.size());
    cm.coupledTo.assign(typeRefs.types.begin(), typeRefs.types.end());
    cm.cbo = static_cast<int>(cm.coupledTo.size());
    for (const auto& name : cm.coupledTo) {
        if (auto it = typeRefs.typeSyms.find(name); it != typeRefs.typeSyms.end())
            cm.coupledClasses.push_back(it->second);
    }

    // TCC over methods that touch fields: share of pairs with a common field.
    size_t n = uses.size();
    if (n >= 2) {
        size_t pairs = 0, connected = 0;
        for (size_t i = 0; i < n; i++) {
            for (size_t j = i + 1; j < n; j++) {
                pairs++;
                for (const auto* f : uses[i].fields) {
                    if (uses[j].fields.count(f)) { connected++; break; }
                }
            }
        }
        cm.tcc = pairs ? static_cast<double>(connected) / static_cast<double>(pairs) : 1.0;
    }

    // LCOM4: connected components of methods linked by shared fields or calls.
    UnionFind uf(n);
    std::map<const SubroutineSymbol*, int> index;
    for (size_t i = 0; i < n; i++) index[uses[i].sub] = static_cast<int>(i);
    for (size_t i = 0; i < n; i++) {
        for (size_t j = i + 1; j < n; j++) {
            for (const auto* f : uses[i].fields) {
                if (uses[j].fields.count(f)) { uf.join(static_cast<int>(i), static_cast<int>(j)); break; }
            }
        }
        for (const auto* callee : uses[i].calls) {
            if (auto it = index.find(callee); it != index.end()) uf.join(static_cast<int>(i), it->second);
        }
    }
    std::map<int, std::vector<std::string>> groups;
    for (size_t i = 0; i < n; i++) groups[uf.find(static_cast<int>(i))].push_back(std::string(uses[i].sub->name));
    cm.lcom4 = n ? static_cast<int>(groups.size()) : 1;
    for (auto& [_, names] : groups) cm.lcomGroups.push_back(std::move(names));
    return cm;
}

} // namespace

RoutineMetrics computeRoutineMetrics(const SubroutineSymbol& sub) {
    RoutineWalker w;
    w.walk(sub.getBody(), 0);
    w.m.statements = swe::countStatements(sub.getBody());
    w.m.arguments = static_cast<int>(sub.getArguments().size());
    return w.m;
}

const std::vector<ClassMetrics>& classMetrics(Compilation& comp, const uvm::UvmModel& model) {
    static std::mutex mu;
    static const Compilation* cachedFor = nullptr;
    static std::vector<ClassMetrics> cache;
    std::lock_guard<std::mutex> lock(mu);
    if (cachedFor != &comp) {
        cache.clear();
        for (const auto* cls : swe::userClasses(model)) cache.push_back(computeClass(*cls));
        cachedFor = &comp;
    }
    return cache;
}

} // namespace uvmcheck::rules::metrics
