#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/Constraints.h"
#include "slang/ast/EvalContext.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include <map>
#include <optional>
#include <string>
#include <unordered_set>

namespace uvmcheck::rules::random {

using namespace slang::ast;

// --- SV-RND-024: randomize() as the condition of an immediate assertion -------------

// True when the condition is a randomize() call, alone or joined by && / ||.
bool callsRandomize(const Expression& expr) {
    const Expression& e = swe::stripConversions(expr);
    if (e.kind == ExpressionKind::Call) return e.as<CallExpression>().getSubroutineName() == "randomize";
    if (e.kind == ExpressionKind::BinaryOp) {
        const auto& b = e.as<BinaryExpression>();
        if (b.op == BinaryOperator::LogicalAnd || b.op == BinaryOperator::LogicalOr) {
            return callsRandomize(b.left()) || callsRandomize(b.right());
        }
    }
    return false;
}

class RandomizeInsideAssertVisitor : public swe::UserCodeVisitor<RandomizeInsideAssertVisitor> {
public:
    using UserCodeVisitor::handle;
    explicit RandomizeInsideAssertVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const ImmediateAssertionStatement& stmt) {
        if ((stmt.assertionKind == AssertionKind::Assert || stmt.assertionKind == AssertionKind::Assume) &&
            callsRandomize(stmt.cond)) {
            diags.addViolation(
                "SV-RND-024", "RandomizeInsideAssert",
                "randomize() is called inside an assertion. When assertions are turned off "
                "($assertoff or a simulator switch) the call is skipped and the object keeps its old "
                "values. Call it on its own: 'if (!obj.randomize()) `uvm_fatal(...)'.",
                core::Severity::Warning, stmt.sourceRange.start());
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class RandomizeInsideAssert : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-024"; }
    std::string_view getName() const override { return "RandomizeInsideAssert"; }
    std::string_view getDescription() const override {
        return "Flags randomize() used as the condition of an immediate assert or assume. Turning "
               "assertions off also skips the randomization, so the object silently keeps stale values.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        RandomizeInsideAssertVisitor visitor(ctx.diags());
        co_await ctx.visitAst(visitor);
    }
};

// --- Constraint walking shared by SV-RND-025 and SV-RND-026 ------------------------

template<typename F>
void forEachConstraint(const Constraint& c, F&& f) {
    f(c);
    switch (c.kind) {
        case ConstraintKind::List:
            for (const auto* item : c.as<ConstraintList>().list) {
                if (item) forEachConstraint(*item, f);
            }
            break;
        case ConstraintKind::Implication:
            forEachConstraint(c.as<ImplicationConstraint>().body, f);
            break;
        case ConstraintKind::Conditional: {
            const auto& cond = c.as<ConditionalConstraint>();
            forEachConstraint(cond.ifBody, f);
            if (cond.elseBody) forEachConstraint(*cond.elseBody, f);
            break;
        }
        case ConstraintKind::Foreach:
            forEachConstraint(c.as<ForeachConstraint>().body, f);
            break;
        default:
            break;
    }
}

template<typename F>
void forEachUserConstraintBlock(const uvm::UvmModel& model, F&& f) {
    for (const auto* cls : model.getAllClasses()) {
        if (!cls || swe::isLibrarySymbol(*cls)) continue;
        for (const auto& member : cls->members()) {
            if (member.kind == SymbolKind::ConstraintBlock) f(*cls, member.as<ConstraintBlockSymbol>());
        }
    }
}

// --- SV-RND-025: '[lo:hi] := w' over a wide range in a dist ------------------------

std::optional<int64_t> constantInt(const Expression& expr, EvalContext& ec) {
    const Expression& e = swe::stripConversions(expr);
    if (e.kind == ExpressionKind::UnboundedLiteral) return std::nullopt;
    slang::ConstantValue cv = e.getConstant() ? *e.getConstant() : e.eval(ec);
    if (!cv.isInteger() || cv.integer().hasUnknown()) return std::nullopt;
    return cv.integer().as<int64_t>();
}

// Number of values a dist item covers, or nullopt for a single value or an unknown range.
std::optional<int64_t> rangeWidth(const Expression& value, EvalContext& ec) {
    const Expression& e = swe::stripConversions(value);
    if (e.kind != ExpressionKind::ValueRange) return std::nullopt;
    const auto& r = e.as<ValueRangeExpression>();
    if (r.rangeKind != ValueRangeKind::Simple) return std::nullopt;
    auto lo = constantInt(r.left(), ec);
    auto hi = constantInt(r.right(), ec);
    if (!lo || !hi) return std::nullopt;
    return (*hi >= *lo ? *hi - *lo : *lo - *hi) + 1;
}

void checkDist(const DistExpression& dist, const Symbol& scope, int64_t limit, core::DiagnosticEngine& diags) {
    EvalContext ec(ASTContext(*scope.getParentScope(), LookupLocation::max));

    // A dist made only of same-width ranges is an intentional uniform split; := is fine there.
    std::optional<int64_t> commonWidth;
    bool uniform = true;
    for (const auto& item : dist.items()) {
        auto w = rangeWidth(item.value, ec);
        if (!w || (commonWidth && *commonWidth != *w)) { uniform = false; break; }
        commonWidth = w;
    }
    if (uniform) return;

    for (const auto& item : dist.items()) {
        if (!item.weight || item.weight->kind != DistExpression::DistWeight::PerValue) continue;
        auto w = rangeWidth(item.value, ec);
        if (!w || *w <= limit) continue;
        diags.addViolation(
            "SV-RND-025", "DistWideRangeColonEq",
            "':=' gives every one of the " + std::to_string(*w) +
                " values in this range the full weight, so the range outweighs the single values "
                "next to it. Use ':/' to share the weight across the range, or write the per-value "
                "weight on purpose.",
            core::Severity::Warning, item.value.sourceRange.start());
    }
}

class DistWideRangeColonEq : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-025"; }
    std::string_view getName() const override { return "DistWideRangeColonEq"; }
    std::string_view getDescription() const override {
        return "Flags '[lo:hi] := w' in a dist when the range holds more than max_range_values "
               "(default 16) values and the dist also has items of another size. ':=' applies the "
               "weight to each value, which is rarely what the author meant; ':/' shares it.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-RND-025", "max_range_values", 16);
        forEachUserConstraintBlock(model, [&](const ClassType&, const ConstraintBlockSymbol& cb) {
            forEachConstraint(cb.getConstraints(), [&](const Constraint& c) {
                if (c.kind != ConstraintKind::Expression) return;
                const Expression& e = swe::stripConversions(c.as<ExpressionConstraint>().expr);
                if (e.kind == ExpressionKind::Dist) checkDist(e.as<DistExpression>(), cb, limit, diags);
            });
        });
    }
};

// --- SV-RND-026: solve-before between variables no constraint relates ---------------

// Union-find over symbols: variables named in the same constraint item are related.
class Relations {
public:
    void relate(const std::unordered_set<const Symbol*>& syms) {
        const Symbol* first = nullptr;
        for (const auto* s : syms) {
            if (!first) { first = s; find(s); continue; }
            parent[find(s)] = find(first);
        }
    }
    bool related(const Symbol* a, const Symbol* b) {
        return parent.count(a) && parent.count(b) && find(a) == find(b);
    }

private:
    const Symbol* find(const Symbol* s) {
        auto it = parent.find(s);
        if (it == parent.end()) return parent[s] = s;
        if (it->second == s) return s;
        return it->second = find(it->second);
    }
    std::map<const Symbol*, const Symbol*> parent;
};

void addSymbols(const Expression& e, std::unordered_set<const Symbol*>& out) {
    for (const auto* s : swe::referencedSymbols(e)) out.insert(s);
}

// Every variable a constraint item names, including the guards of implications and ifs.
void collectGroup(const Constraint& c, std::unordered_set<const Symbol*>& out) {
    switch (c.kind) {
        case ConstraintKind::Expression:
            addSymbols(c.as<ExpressionConstraint>().expr, out);
            break;
        case ConstraintKind::Implication: {
            const auto& imp = c.as<ImplicationConstraint>();
            addSymbols(imp.predicate, out);
            collectGroup(imp.body, out);
            break;
        }
        case ConstraintKind::Conditional: {
            const auto& cond = c.as<ConditionalConstraint>();
            addSymbols(cond.predicate, out);
            collectGroup(cond.ifBody, out);
            if (cond.elseBody) collectGroup(*cond.elseBody, out);
            break;
        }
        case ConstraintKind::Uniqueness:
            for (const auto* e : c.as<UniquenessConstraint>().items) {
                if (e) addSymbols(*e, out);
            }
            break;
        case ConstraintKind::Foreach: {
            const auto& fe = c.as<ForeachConstraint>();
            addSymbols(fe.arrayRef, out);
            collectGroup(fe.body, out);
            break;
        }
        case ConstraintKind::List:
            for (const auto* item : c.as<ConstraintList>().list) {
                if (item) collectGroup(*item, out);
            }
            break;
        default:
            break;
    }
}

// Top-level items of a block each form one group; a solve-before item forms none.
void relateBlock(const Constraint& c, Relations& rel) {
    if (c.kind == ConstraintKind::List) {
        for (const auto* item : c.as<ConstraintList>().list) {
            if (item) relateBlock(*item, rel);
        }
        return;
    }
    if (c.kind == ConstraintKind::SolveBefore) return;
    std::unordered_set<const Symbol*> group;
    collectGroup(c, group);
    rel.relate(group);
}

class UnrelatedSolveBefore : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-026"; }
    std::string_view getName() const override { return "UnrelatedSolveBefore"; }
    std::string_view getDescription() const override {
        return "Flags 'solve a before b' when no constraint of the class (or its base classes) ties "
               "a to b, directly or through other variables. The ordering then has no effect except "
               "to suggest a dependency that does not exist.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : model.getAllClasses()) {
            if (!cls || swe::isLibrarySymbol(*cls)) continue;

            // Relations come from this class and every base class, since constraints inherit.
            Relations rel;
            for (const ClassType* c = cls; c; c = baseOf(*c)) {
                for (const auto& member : c->members()) {
                    if (member.kind == SymbolKind::ConstraintBlock) {
                        relateBlock(member.as<ConstraintBlockSymbol>().getConstraints(), rel);
                    }
                }
            }

            for (const auto& member : cls->members()) {
                if (member.kind != SymbolKind::ConstraintBlock) continue;
                forEachConstraint(member.as<ConstraintBlockSymbol>().getConstraints(), [&](const Constraint& c) {
                    if (c.kind != ConstraintKind::SolveBefore) return;
                    const auto& sb = c.as<SolveBeforeConstraint>();
                    for (const auto* a : sb.solve) {
                        for (const auto* b : sb.after) {
                            if (!a || !b) continue;
                            const Symbol* sa = swe::rootSymbol(*a);
                            const Symbol* sb2 = swe::rootSymbol(*b);
                            if (!sa || !sb2 || sa == sb2 || rel.related(sa, sb2)) continue;
                            diags.addViolation(
                                "SV-RND-026", "UnrelatedSolveBefore",
                                "'solve " + std::string(sa->name) + " before " + std::string(sb2->name) +
                                    "': no constraint relates these variables, so the order changes "
                                    "nothing. Remove it, or add the constraint it was meant to shape.",
                                core::Severity::Warning, a->sourceRange.start());
                        }
                    }
                });
            }
        }
    }

private:
    static const ClassType* baseOf(const ClassType& cls) {
        const Type* base = cls.getBaseClass();
        if (!base) return nullptr;
        const auto* bc = swe::asClass(*base);
        return bc && !swe::isLibrarySymbol(*bc) ? bc : nullptr;
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRandomizeInsideAssert() {
    return std::make_unique<random::RandomizeInsideAssert>();
}
std::unique_ptr<Rule> createDistWideRangeColonEq() {
    return std::make_unique<random::DistWideRangeColonEq>();
}
std::unique_ptr<Rule> createUnrelatedSolveBefore() {
    return std::make_unique<random::UnrelatedSolveBefore>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createRandomizeInsideAssert)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createDistWideRangeColonEq)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnrelatedSolveBefore)
