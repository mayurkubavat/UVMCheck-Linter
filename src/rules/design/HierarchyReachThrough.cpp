#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"

#include <unordered_set>

namespace uvmcheck::rules::design {

using namespace slang::ast;

namespace {

bool isComponentType(const Type* t) {
    if (!t) return false;
    const ClassType* c = swe::asClass(*t);
    return c && swe::isComponentRole(swe::classRole(*c));
}

bool isSequencerType(const Type* t) {
    if (!t) return false;
    const ClassType* c = swe::asClass(*t);
    if (!c) return false;
    std::string_view n = c->genericClass ? c->genericClass->name : c->name;
    return swe::classRole(*c) == swe::Role::Sequencer || n.starts_with("uvm_sequencer");
}

const Expression& skipSelects(const Expression& e) {
    const Expression* x = &swe::stripConversions(e);
    while (x->kind == ExpressionKind::ElementSelect) x = &swe::stripConversions(x->as<ElementSelectExpression>().value());
    return *x;
}

class ReachVisitor : public ASTVisitor<ReachVisitor, VisitFlags::AllGood> {
public:
    ReachVisitor(const std::string& where, core::DiagnosticEngine& diags) : where(where), diags(diags) {}

    void handle(const MemberAccessExpression& e) {
        if (!inner.count(&e)) check(e);
        visitDefault(e);
    }

private:
    const std::string& where;
    core::DiagnosticEngine& diags;
    std::unordered_set<const Expression*> inner;

    void check(const MemberAccessExpression& top) {
        // Walk down the chain a.b[0].c.d, counting component handles along the way.
        int hops = 0;
        const Expression* e = &top.value();
        while (true) {
            const Expression& x = skipSelects(*e);
            // Count the selected element's type, so agt[0] counts as a component handle.
            if (isComponentType(swe::stripConversions(*e).type)) {
                bool isThis = x.kind == ExpressionKind::NamedValue && x.as<NamedValueExpression>().symbol.name == "this";
                if (!isThis) hops++;
            }
            if (x.kind != ExpressionKind::MemberAccess) break;
            inner.insert(&x);
            e = &x.as<MemberAccessExpression>().value();
        }
        if (hops < 3) return;
        // seq.start(env.agt.sqr) is how tests pick a sequencer; allow it.
        if (isSequencerType(top.type)) return;
        diags.addViolation(
            "UVM-ARC-002", "HierarchyReachThrough",
            "'" + swe::exprText(top) + "' reaches through " + std::to_string(hops) +
                " component handles from " + where +
                ". It breaks as soon as the env is restructured and couples this code to other "
                "components' internals (Law of Demeter). Pass what is needed through config_db, a "
                "config object or a TLM connection.",
            core::Severity::Warning, swe::locOf(top));
    }
};

} // namespace

class HierarchyReachThrough : public Rule {
public:
    std::string_view getId() const override { return "UVM-ARC-002"; }
    std::string_view getName() const override { return "HierarchyReachThrough"; }
    std::string_view getDescription() const override {
        return "Flags expressions that reach through three or more component handles (env.agt.drv.vif), "
               "outside connect_phase wiring (PMD LawOfDemeter, Fowler 'message chains').";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                // Wiring phases legitimately reach into children.
                if (m->name == "connect_phase" || m->name == "end_of_elaboration_phase") continue;
                std::string where = std::string(cls->name) + "::" + std::string(m->name);
                ReachVisitor v(where, diags);
                m->getBody().visit(v);
            }
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createHierarchyReachThrough() {
    return std::make_unique<design::HierarchyReachThrough>();
}
}
