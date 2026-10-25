// Methodology guide chapters MG-5 (design-pattern conformance) and MG-8 (failure handling):
// UVM-PAT-001, RAL-ADP-001, UVM-REP-007 and UVM-REP-008.

#include "MethodologySupport.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/ParameterSymbols.h"

#include <cctype>

namespace uvmcheck::rules::methodology {

using namespace slang::ast;

namespace {

// The class bound to a type parameter of a specialization: T of uvm_callbacks#(T, CB).
const ClassType* typeParam(const ClassType& spec, std::string_view name) {
    for (const auto& m : spec.members()) {
        if (m.kind == SymbolKind::TypeParameter && m.name == name) {
            return swe::asClass(m.as<TypeParameterSymbol>().targetType.getType());
        }
    }
    return nullptr;
}

bool sameOrDerived(const ClassType& cls, const ClassType& base) {
    return &cls.getCanonicalType() == &base.getCanonicalType() || swe::derivesFrom(cls, base);
}

struct CbPair {
    const ClassType* t = nullptr;
    const ClassType* cb = nullptr;
};

// `uvm_register_cb(T, CB) declares a static m_register_cb_<CB> initialized by
// uvm_callbacks#(T, CB)::m_register_pair().
std::vector<CbPair> registrations(const std::vector<const ClassType*>& classes) {
    std::vector<CbPair> out;
    for (const auto* cls : classes) {
        for (const auto* p : meth::ownProperties(*cls)) {
            if (!p->name.starts_with("m_register_cb_")) continue;
            const Expression* init = p->getInitializer();
            if (!init) continue;
            const Expression& e = swe::stripConversions(*init);
            if (e.kind != ExpressionKind::Call) continue;
            const auto& call = e.as<CallExpression>();
            const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine);
            if (!sub || !*sub || !(*sub)->getParentScope()) continue;
            const Symbol& owner = (*sub)->getParentScope()->asSymbol();
            if (owner.kind != SymbolKind::ClassType) continue;
            const auto& spec = owner.as<ClassType>();
            CbPair pair{typeParam(spec, "T"), typeParam(spec, "CB")};
            if (pair.t && pair.cb) out.push_back(pair);
        }
    }
    return out;
}

class CallbackUseVisitor : public ASTVisitor<CallbackUseVisitor, VisitFlags::AllGood> {
public:
    struct Use {
        const ClassType* t;
        const ClassType* cb;
        slang::SourceLocation loc;
    };
    std::vector<Use> uses;

    void handle(const VariableDeclStatement& s) {
        if (const ClassType* c = swe::asClass(s.symbol.getType())) {
            if (meth::plainName(*c) == "uvm_callback_iter") {
                const ClassType* t = typeParam(*c, "T");
                const ClassType* cb = typeParam(*c, "CB");
                if (t && cb) uses.push_back({t, cb, s.symbol.location});
            }
        }
        visitDefault(s);
    }
};

} // namespace

// UVM-PAT-001 -----------------------------------------------------------------------------

class CallbackNotRegistered : public Rule {
public:
    std::string_view getId() const override { return "UVM-PAT-001"; }
    std::string_view getName() const override { return "CallbackNotRegistered"; }
    std::string_view getDescription() const override {
        return "Flags `uvm_do_callbacks(T, CB, ...) in classes where no `uvm_register_cb(T, CB) "
               "registers that pair for T or a base of T. UVM then refuses to add callbacks of that "
               "type at run time, so the hook never fires (MG-5.1).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        auto classes = swe::userClasses(model);
        auto regs = registrations(classes);
        for (const auto* cls : classes) {
            CallbackUseVisitor v;
            for (const auto* m : swe::classMethods(*cls)) m->getBody().visit(v);
            for (const auto& use : v.uses) {
                bool registered = false;
                for (const auto& r : regs) {
                    registered |= sameOrDerived(*use.t, *r.t) && sameOrDerived(*use.cb, *r.cb);
                }
                if (registered) continue;
                std::string t(meth::plainName(*use.t)), cb(meth::plainName(*use.cb));
                diags.addViolation(
                    "UVM-PAT-001", "CallbackNotRegistered",
                    "'" + std::string(cls->name) + "' runs callbacks of type '" + cb + "' for '" + t +
                        "', but no class registers the pair with `uvm_register_cb(" + t + ", " + cb +
                        "). uvm_callbacks::add() rejects unregistered pairs, so these callbacks are "
                        "never called. Add `uvm_register_cb(" + t + ", " + cb + ") to '" + t + "'.",
                    core::Severity::Warning, use.loc);
            }
        }
    }
};

// RAL-ADP-001 -----------------------------------------------------------------------------

namespace {

// Uses of a formal argument: member names read or written through it, and whether it is
// assigned whole or handed to another routine (which may fill it in).
class ArgUseVisitor : public ASTVisitor<ArgUseVisitor, VisitFlags::AllGood> {
public:
    explicit ArgUseVisitor(const Symbol& arg) : arg(arg) {}
    std::vector<std::string_view> writtenMembers, readMembers;
    bool wholeUse = false;

    void handle(const AssignmentExpression& a) {
        const Expression& lhs = swe::stripConversions(a.left());
        if (lhs.kind == ExpressionKind::MemberAccess && isArg(lhs.as<MemberAccessExpression>().value())) {
            writtenMembers.push_back(lhs.as<MemberAccessExpression>().member.name);
            a.right().visit(*this);
            return;
        }
        if (isArg(lhs)) wholeUse = true;
        visitDefault(a);
    }
    void handle(const MemberAccessExpression& e) {
        if (isArg(e.value())) readMembers.push_back(e.member.name);
        visitDefault(e);
    }
    void handle(const CallExpression& call) {
        for (const auto* actual : call.arguments()) {
            if (isArg(*actual)) wholeUse = true;
        }
        visitDefault(call);
    }

private:
    const Symbol& arg;
    bool isArg(const Expression& e) const {
        const Expression& x = swe::stripConversions(e);
        return x.kind == ExpressionKind::NamedValue && &x.as<NamedValueExpression>().symbol == &arg;
    }
};

bool contains(const std::vector<std::string_view>& v, std::string_view s) {
    for (auto x : v) if (x == s) return true;
    return false;
}

} // namespace

class IncompleteRegAdapter : public Rule {
public:
    std::string_view getId() const override { return "RAL-ADP-001"; }
    std::string_view getName() const override { return "IncompleteRegAdapter"; }
    std::string_view getDescription() const override {
        return "Flags uvm_reg_adapter subclasses whose bus2reg() never sets rw.status or whose "
               "reg2bus() never reads rw.kind. The register layer then reports every access as passed, "
               "or turns reads into writes (adapter pattern, MG-5.1).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::uvmBaseName(*cls) != "uvm_reg_adapter") continue;
            for (const auto* m : swe::classMethods(*cls)) {
                auto args = m->getArguments();
                if (m->name == "bus2reg" && args.size() >= 2) {
                    ArgUseVisitor v(*args[1]);
                    m->getBody().visit(v);
                    if (v.wholeUse || contains(v.writtenMembers, "status")) continue;
                    report(diags, *cls, *m,
                           "bus2reg() never sets rw.status, so every register access is reported as "
                           "UVM_IS_OK even when the bus returned an error. Copy the bus response "
                           "status into rw.status.");
                } else if (m->name == "reg2bus" && !args.empty()) {
                    ArgUseVisitor v(*args[0]);
                    m->getBody().visit(v);
                    if (v.wholeUse || contains(v.readMembers, "kind")) continue;
                    report(diags, *cls, *m,
                           "reg2bus() never reads rw.kind, so reads and writes become the same bus "
                           "transaction. Set the item's direction from rw.kind.");
                }
            }
        }
    }

private:
    static void report(core::DiagnosticEngine& diags, const ClassType& cls, const SubroutineSymbol& m,
                       const std::string& why) {
        diags.addViolation("RAL-ADP-001", "IncompleteRegAdapter",
                           "Register adapter '" + std::string(cls.name) + "': " + why,
                           core::Severity::Warning, m.location);
    }
};

// UVM-REP-007 / UVM-REP-008 ---------------------------------------------------------------

class CheckFailureAsWarning : public Rule {
public:
    std::string_view getId() const override { return "UVM-REP-007"; }
    std::string_view getName() const override { return "CheckFailureAsWarning"; }
    std::string_view getDescription() const override {
        return "Flags a failed compare() or an expected-versus-actual mismatch reported only with "
               "`uvm_warning or `uvm_info. The test still passes; check failures must be errors so "
               "they are counted (MG-8.1).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                for (const auto& fb : meth::failureBranches(m->getBody())) {
                    swe::ReportLevel lvl = swe::strongestReport(*fb.branch);
                    if (lvl != swe::ReportLevel::Warning && lvl != swe::ReportLevel::Info) continue;
                    diags.addViolation(
                        "UVM-REP-007", "CheckFailureAsWarning",
                        "A failed check (" + fb.what + ") in '" + std::string(cls->name) + "::" +
                            std::string(m->name) + "' is reported with `uvm_" +
                            (lvl == swe::ReportLevel::Warning ? "warning" : "info") +
                            ". Warnings do not fail the test, so the mismatch goes unnoticed in "
                            "regression; report it with `uvm_error.",
                        core::Severity::Warning, swe::locOf(*fb.condition));
                }
            }
        }
    }
};

class FatalInCheckerPath : public Rule {
public:
    std::string_view getId() const override { return "UVM-REP-008"; }
    std::string_view getName() const override { return "FatalInCheckerPath"; }
    std::string_view getDescription() const override {
        return "Flags `uvm_fatal on a data mismatch in scoreboards and subscribers. The run stops at "
               "the first mismatch and hides how many others there are; use `uvm_error and let "
               "max_quit_count stop the run (MG-8.1).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            swe::Role role = swe::classRole(*cls);
            if (role != swe::Role::Scoreboard && role != swe::Role::Subscriber) continue;
            for (const auto* m : swe::classMethods(*cls)) {
                for (const auto& fb : meth::failureBranches(m->getBody())) {
                    if (swe::strongestReport(*fb.branch) != swe::ReportLevel::Fatal) continue;
                    diags.addViolation(
                        "UVM-REP-008", "FatalInCheckerPath",
                        std::string(swe::roleName(role)) + " '" + std::string(cls->name) +
                            "' stops the run with `uvm_fatal when a check (" + fb.what + ") fails in " +
                            std::string(m->name) + "(). One mismatch then hides all later ones and "
                            "skips the report phase summary; report it with `uvm_error.",
                        core::Severity::Warning, swe::locOf(*fb.condition));
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCallbackNotRegistered() { return std::make_unique<methodology::CallbackNotRegistered>(); }
std::unique_ptr<Rule> createIncompleteRegAdapter() { return std::make_unique<methodology::IncompleteRegAdapter>(); }
std::unique_ptr<Rule> createCheckFailureAsWarning() { return std::make_unique<methodology::CheckFailureAsWarning>(); }
std::unique_ptr<Rule> createFatalInCheckerPath() { return std::make_unique<methodology::FatalInCheckerPath>(); }
}
