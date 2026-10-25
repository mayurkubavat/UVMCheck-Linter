// Methodology guide chapter MG-2, coupling: UVM-CPL-002 to UVM-CPL-004.

#include "MethodologySupport.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/symbols/InstanceSymbols.h"

namespace uvmcheck::rules::methodology {

using namespace slang::ast;

// UVM-CPL-002 -----------------------------------------------------------------------------

class HandleToEnvOrTest : public Rule {
public:
    std::string_view getId() const override { return "UVM-CPL-002"; }
    std::string_view getName() const override { return "HandleToEnvOrTest"; }
    std::string_view getDescription() const override {
        return "Flags sequences, items, agents, config objects and helpers that store a handle to a "
               "user env or test class. They then depend on everything the env contains and cannot be "
               "used under another env (stamp coupling, dependency inversion, MG-2.1).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            swe::Role role = swe::classRole(*cls);
            if (role == swe::Role::Env || role == swe::Role::Test) continue;
            for (const auto* p : meth::ownProperties(*cls)) {
                const ClassType* t = meth::classOfType(p->getType());
                if (!t || swe::isLibrarySymbol(*t)) continue;
                swe::Role r = swe::classRole(*t);
                if (r != swe::Role::Env && r != swe::Role::Test) continue;
                diags.addViolation(
                    "UVM-CPL-002", "HandleToEnvOrTest",
                    std::string(swe::roleName(role)) + " '" + std::string(cls->name) + "' stores '" +
                        std::string(p->name) + "', a handle to " + swe::roleName(r) + " '" +
                        std::string(meth::plainName(*t)) + "'. Everything below the env now depends "
                        "on the env's structure. Pass in only what is used: a config object, a "
                        "sequencer handle or a TLM port.",
                    core::Severity::Warning, p->location);
            }
        }
    }
};

// UVM-CPL-003 -----------------------------------------------------------------------------

namespace {

const Expression& skipSelects(const Expression& e) {
    const Expression* x = &swe::stripConversions(e);
    while (true) {
        if (x->kind == ExpressionKind::ElementSelect) x = &swe::stripConversions(x->as<ElementSelectExpression>().value());
        else if (x->kind == ExpressionKind::RangeSelect) x = &swe::stripConversions(x->as<RangeSelectExpression>().value());
        else return *x;
    }
}

bool isThisOrSuper(const Expression& e) {
    const Expression& x = skipSelects(e);
    if (x.kind != ExpressionKind::NamedValue) return false;
    std::string_view n = x.as<NamedValueExpression>().symbol.name;
    return n == "this" || n == "super";
}

// A descendant reached from one of `cls`'s own (or inherited) properties: agt, agt[0],
// this.agt, agt.drv ...
bool reachedThroughOwnChild(const Expression& e, const ClassType& cls) {
    const Expression* x = &skipSelects(e);
    while (x->kind == ExpressionKind::MemberAccess) {
        const auto& ma = x->as<MemberAccessExpression>();
        if (isThisOrSuper(ma.value())) break;
        x = &skipSelects(ma.value());
    }
    const Symbol* sym = nullptr;
    if (x->kind == ExpressionKind::NamedValue) sym = &x->as<NamedValueExpression>().symbol;
    else if (x->kind == ExpressionKind::MemberAccess) sym = &x->as<MemberAccessExpression>().member;
    if (!sym || sym->kind != SymbolKind::ClassProperty) return false;
    const Scope* owner = sym->getParentScope();
    if (!owner || owner->asSymbol().kind != SymbolKind::ClassType) return false;
    const auto& ownerCls = owner->asSymbol().as<ClassType>();
    return &ownerCls == &cls || swe::derivesFrom(cls, ownerCls);
}

bool isBuildTime(std::string_view method) {
    return method == "build_phase" || method == "connect_phase" || method == "end_of_elaboration_phase" ||
           method == "new";
}

class ForeignWriteVisitor : public ASTVisitor<ForeignWriteVisitor, VisitFlags::AllGood> {
public:
    ForeignWriteVisitor(const ClassType& cls, const SubroutineSymbol& method, core::DiagnosticEngine& diags) :
        cls(cls), method(method), diags(diags) {}

    void handle(const AssignmentExpression& a) {
        check(a.left());
        visitDefault(a);
    }

private:
    const ClassType& cls;
    const SubroutineSymbol& method;
    core::DiagnosticEngine& diags;

    void check(const Expression& lhs) {
        const Expression& target = skipSelects(lhs);
        if (target.kind != ExpressionKind::MemberAccess) return;
        const auto& ma = target.as<MemberAccessExpression>();
        if (ma.member.kind != SymbolKind::ClassProperty) return;
        const Expression& obj = ma.value();
        if (!obj.type || isThisOrSuper(obj)) return;
        const ClassType* objCls = meth::classOfType(*obj.type);
        if (!objCls || !swe::isComponentRole(swe::classRole(*objCls))) return;
        // A parent setting up its own subtree while the hierarchy is built is wiring.
        if (isBuildTime(method.name) && reachedThroughOwnChild(obj, cls)) return;
        std::string objText = swe::exprText(obj);
        // p_sequencer / m_sequencer access is UVM-SEQ-003's finding.
        if (objText == "p_sequencer" || objText == "m_sequencer") return;
        diags.addViolation(
            "UVM-CPL-003", "ForeignComponentStateWrite",
            "'" + std::string(cls.name) + "::" + std::string(method.name) + "' writes '" +
                swe::exprText(target) + "', state that belongs to " +
                swe::roleName(swe::classRole(*objCls)) + " '" + std::string(meth::plainName(*objCls)) +
                "'. The other component can no longer change how it keeps that state, and the write "
                "is invisible from its code. Send the value through its config object, a TLM port or "
                "a method it provides.",
            core::Severity::Warning, swe::locOf(target));
    }
};

} // namespace

class ForeignComponentStateWrite : public Rule {
public:
    std::string_view getId() const override { return "UVM-CPL-003"; }
    std::string_view getName() const override { return "ForeignComponentStateWrite"; }
    std::string_view getDescription() const override {
        return "Flags assignments to properties of another component through a handle, except a "
               "parent setting up its own subtree in new() and the build, connect and "
               "end_of_elaboration phases (content coupling, MG-2.1). UVM-ARC-002 covers long read "
               "chains.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                ForeignWriteVisitor v(*cls, *m, diags);
                m->getBody().visit(v);
            }
        }
    }
};

// UVM-CPL-004 -----------------------------------------------------------------------------

namespace {

bool insideModuleHierarchy(const Symbol& sym) {
    for (const Scope* s = sym.getParentScope(); s; s = s->asSymbol().getParentScope()) {
        if (s->asSymbol().kind == SymbolKind::InstanceBody) return true;
    }
    return false;
}

class HierRefVisitor : public ASTVisitor<HierRefVisitor, VisitFlags::AllGood> {
public:
    HierRefVisitor(std::string where, core::DiagnosticEngine& diags) : where(std::move(where)), diags(diags) {}

    void handle(const HierarchicalValueExpression& e) {
        if (insideModuleHierarchy(e.symbol)) {
            diags.addViolation(
                "UVM-CPL-004", "HierarchicalReferenceInClass",
                where + " reads '" + swe::exprText(e) + "' through the module hierarchy. The class "
                    "then only compiles and works in one testbench top; pass signals in through a "
                    "virtual interface from config_db instead.",
                core::Severity::Warning, swe::locOf(e));
        }
        visitDefault(e);
    }

private:
    std::string where;
    core::DiagnosticEngine& diags;
};

} // namespace

class HierarchicalReferenceInClass : public Rule {
public:
    std::string_view getId() const override { return "UVM-CPL-004"; }
    std::string_view getName() const override { return "HierarchicalReferenceInClass"; }
    std::string_view getDescription() const override {
        return "Flags class code that names a module hierarchy (tb_top.dut.sig, $root.tb.x). Classes "
               "must reach the DUT only through virtual interfaces so they stay reusable (information "
               "hiding, MG-2.2).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                HierRefVisitor v(std::string(cls->name) + "::" + std::string(m->name), diags);
                m->getBody().visit(v);
            }
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createHandleToEnvOrTest() { return std::make_unique<methodology::HandleToEnvOrTest>(); }
std::unique_ptr<Rule> createForeignComponentStateWrite() { return std::make_unique<methodology::ForeignComponentStateWrite>(); }
std::unique_ptr<Rule> createHierarchicalReferenceInClass() { return std::make_unique<methodology::HierarchicalReferenceInClass>(); }
}
