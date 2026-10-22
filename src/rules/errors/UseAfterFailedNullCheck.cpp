#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/PathWalker.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"

#include <map>
#include <set>

namespace uvmcheck::rules::errors {

using namespace slang::ast;

namespace {

// Handles known to be null on the current path, with where that became known.
struct NullSet {
    std::map<const Symbol*, slang::SourceLocation> syms;
    bool operator==(const NullSet& o) const {
        if (syms.size() != o.syms.size()) return false;
        for (const auto& [k, _] : syms) if (!o.syms.count(k)) return false;
        return true;
    }
};

bool isHandleType(const Type& t) {
    const Type& c = t.getCanonicalType();
    return c.isClass() || c.isVirtualInterface();
}

const Symbol* trackedSymbol(const Expression& expr) {
    const Expression& e = swe::stripConversions(expr);
    if (e.kind != ExpressionKind::NamedValue && e.kind != ExpressionKind::MemberAccess) return nullptr;
    if (!e.type || !isHandleType(*e.type)) return nullptr;
    return swe::referencedSymbol(e);
}

bool isNullLiteral(const Expression& e) { return swe::stripConversions(e).kind == ExpressionKind::NullLiteral; }

// The handle a failing status call leaves unset: the last argument of config_db::get, the
// destination of $cast.
const Symbol* targetOfFailedCall(const Expression& expr) {
    const Expression& e = swe::stripConversions(expr);
    if (e.kind != ExpressionKind::Call) return nullptr;
    const auto& call = e.as<CallExpression>();
    auto args = call.arguments();
    if (args.empty()) return nullptr;
    std::string_view n = call.getSubroutineName();
    const Expression* target = nullptr;
    if (n == "$cast") target = args[0];
    else if (n == "get" && args.size() == 4 && swe::exprText(call).find("uvm_config_db") != std::string::npos)
        target = args[3];
    if (!target) return nullptr;
    const Expression* t = &swe::stripConversions(*target);
    if (t->kind == ExpressionKind::Assignment) t = &t->as<AssignmentExpression>().left();
    return trackedSymbol(*t);
}

struct UseScanner : public ASTVisitor<UseScanner, VisitFlags::AllGood> {
    NullSet* st = nullptr;
    std::vector<std::pair<const Symbol*, const Expression*>> uses;
    std::set<const Symbol*> kills;

    void use(const Expression& base, const Expression& at) {
        if (const Symbol* s = trackedSymbol(base); s && st->syms.count(s)) uses.push_back({s, &at});
    }

    void handle(const MemberAccessExpression& e) {
        use(e.value(), e);
        visitDefault(e);
    }
    void handle(const CallExpression& call) {
        if (call.thisClass()) use(*call.thisClass(), call);
        visitDefault(call);
    }
    void handle(const AssignmentExpression& a) {
        if (const Symbol* s = trackedSymbol(a.left())) {
            kills.insert(s);
            // Visit only the right side, so the assigned handle is not counted as a use.
            a.right().visit(*this);
            return;
        }
        visitDefault(a);
    }
};

struct Hooks {
    const Compilation& comp;
    core::DiagnosticEngine& diags;
    std::set<const Expression*> reported;

    void leaf(const Statement& s, NullSet& st) {
        if (st.syms.empty()) {
            // Still pick up kills so later checks start clean.
            return;
        }
        UseScanner scan;
        scan.st = &st;
        s.visit(scan);
        for (const auto& [sym, at] : scan.uses) {
            auto it = st.syms.find(sym);
            if (it == st.syms.end()) continue;
            if (reported.insert(at).second) {
                diags.addViolation(
                    "SV-ERR-002", "UseAfterFailedNullCheck",
                    "'" + std::string(sym->name) + "' is null on this path: the check at line " +
                        std::to_string(swe::lineOf(comp, it->second)) +
                        " found it null but execution continued (`uvm_error does not stop the test). "
                        "This dereference then crashes the simulator. Use `uvm_fatal, return, or "
                        "assign a default before the use.",
                    core::Severity::Error, swe::locOf(*at));
            }
            st.syms.erase(it); // one report per check
        }
        for (const auto* k : scan.kills) st.syms.erase(k);
    }

    void branch(const Expression& cond, bool taken, NullSet& st) {
        const Expression* e = &swe::stripConversions(cond);
        bool negated = false;
        if (e->kind == ExpressionKind::UnaryOp && e->as<UnaryExpression>().op == UnaryOperator::LogicalNot) {
            negated = true;
            e = &swe::stripConversions(e->as<UnaryExpression>().operand());
        }
        if (e->kind == ExpressionKind::BinaryOp) {
            const auto& b = e->as<BinaryExpression>();
            bool eq = b.op == BinaryOperator::Equality || b.op == BinaryOperator::CaseEquality;
            bool ne = b.op == BinaryOperator::Inequality || b.op == BinaryOperator::CaseInequality;
            if (!eq && !ne) return;
            const Expression* side = isNullLiteral(b.right()) ? &b.left() : isNullLiteral(b.left()) ? &b.right() : nullptr;
            if (!side) return;
            const Symbol* s = trackedSymbol(*side);
            if (!s) return;
            bool isNull = (eq == taken) != negated;
            if (isNull) st.syms.emplace(s, swe::locOf(cond));
            else st.syms.erase(s);
            return;
        }
        if (negated) {
            // if (!uvm_config_db#(T)::get(..., h)) / if (!$cast(h, x)): h unset on the taken side.
            if (const Symbol* s = targetOfFailedCall(*e)) {
                if (taken) st.syms.emplace(s, swe::locOf(cond));
                else st.syms.erase(s);
            }
        }
    }

    void returned(const Statement&, const NullSet&) {}
};

} // namespace

class UseAfterFailedNullCheck : public Rule {
public:
    std::string_view getId() const override { return "SV-ERR-002"; }
    std::string_view getName() const override { return "UseAfterFailedNullCheck"; }
    std::string_view getDescription() const override {
        return "Flags a handle that a null check, a failed uvm_config_db::get() or a failed $cast() "
               "showed to be null, dereferenced later on the same path because the failure branch "
               "did not stop (SpotBugs NP_NULL_ON_SOME_PATH).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* sub : swe::allUserRoutines(compilation, model)) {
            Hooks hooks{compilation, diags, {}};
            swe::PathWalker<NullSet, Hooks> walker(hooks);
            walker.walkBody(sub->getBody(), NullSet{});
        }
    }
};

} // namespace uvmcheck::rules::errors

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUseAfterFailedNullCheck() {
    return std::make_unique<errors::UseAfterFailedNullCheck>();
}
}
