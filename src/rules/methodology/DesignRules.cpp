// Methodology guide, the last open guidelines: UVM-PAT-003 (MG-5.2, extension points),
// SV-DSN-011 (MG-3.1, feature envy) and
// SV-DBC-001 (MG-8.2, interfaces without assertions).

#include "MethodologySupport.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxNode.h"
#include "slang/text/SourceManager.h"

#include <map>
#include <set>
#include <string>
#include <vector>

namespace uvmcheck::rules::methodology {

using namespace slang::ast;

namespace {

bool isVirtual(const SubroutineSymbol& sub) {
    return (sub.flags & MethodFlags::Virtual) || (sub.flags & MethodFlags::Pure);
}

/// The class and its user-written bases, nearest first.
std::vector<const ClassType*> userLineage(const ClassType& cls) {
    std::vector<const ClassType*> out;
    for (const ClassType* c = &cls; c && !swe::isLibrarySymbol(*c);) {
        out.push_back(c);
        const Type* base = c->getBaseClass();
        c = base ? swe::asClass(*base) : nullptr;
    }
    return out;
}

} // namespace

// UVM-PAT-003 -----------------------------------------------------------------------------

class NoExtensionPoint : public Rule {
public:
    std::string_view getId() const override { return "UVM-PAT-003"; }
    std::string_view getName() const override { return "NoExtensionPoint"; }
    std::string_view getDescription() const override {
        return "Flags a driver or monitor whose run_phase is long (25 statements or more; option "
               "min_statements) while the class and its user bases declare no virtual hook methods and "
               "use no callbacks. A test or derived agent cannot then change one step, such as error "
               "injection or timing, without copying the whole run_phase (MG-5.2). Advisory: off by "
               "default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        co_await ctx.standaloneJob();
        const int minStatements = static_cast<int>(swe::ruleOption(getId(), "min_statements", 25));
        for (const auto* cls : swe::userClasses(ctx.uvmModel())) {
            const swe::Role role = swe::classRole(*cls);
            if (role != swe::Role::Driver && role != swe::Role::Monitor) continue;

            const SubroutineSymbol* run = nullptr;
            for (const auto* m : swe::classMethods(*cls)) {
                if (m->name == "run_phase") run = m;
            }
            if (!run) continue;
            const int statements = swe::countStatements(run->getBody());
            if (statements < minStatements) continue;

            bool hooks = false;
            for (const auto* c : userLineage(*cls)) {
                for (const auto* m : swe::classMethods(*c)) {
                    // A virtual method of its own is a hook; overriding a UVM method is not.
                    hooks |= isVirtual(*m) && !meth::overridesBaseMethod(*m) && m->name != "new";
                }
                if (const auto* syn = c->getSyntax()) {
                    const std::string text = syn->toString();
                    hooks |= text.find("uvm_callback") != std::string::npos ||
                             text.find("uvm_do_callbacks") != std::string::npos ||
                             text.find("uvm_register_cb") != std::string::npos;
                }
            }
            if (hooks) continue;

            ctx.diags().addViolation(
                getId(), getName(),
                std::string(role == swe::Role::Driver ? "Driver" : "Monitor") + " '" + std::string(cls->name) +
                    "' does all its work in a " + std::to_string(statements) +
                    "-statement run_phase and offers no virtual hook methods or callbacks. A test or a "
                    "derived agent cannot change one step (error injection, timing, a protocol variant) "
                    "without copying the whole run_phase. Split the steps into virtual methods "
                    "(drive_item, wait_for_reset, collect_transfer) or add a uvm_callback.",
                core::Severity::Info, run->location);
        }
    }
};

// SV-DSN-011 ------------------------------------------------------------------------------

namespace {

/// Field accesses in one method: its own class's properties, and other objects' properties
/// per receiver.
class FieldUseVisitor : public ASTVisitor<FieldUseVisitor, VisitFlags::AllGood> {
public:
    explicit FieldUseVisitor(const ClassType& self) : self(self) {}

    int own = 0;
    struct Foreign {
        int count = 0;
        const ClassType* type = nullptr;
    };
    std::map<std::string, Foreign> foreign;

    void handle(const MemberAccessExpression& e) {
        if (e.member.kind == SymbolKind::ClassProperty) {
            const Expression& recv = swe::stripConversions(e.value());
            // this.x binds to x itself (a NamedValue), so every receiver here is another object.
            if (const ClassType* t = swe::asClass(*recv.type); t && counts(*t)) {
                std::string key = swe::exprText(recv);
                if (key.empty()) {
                    const Symbol* sym = swe::referencedSymbol(recv);
                    key = sym ? std::string(sym->name) : std::string(t->name);
                }
                auto& f = foreign[key];
                ++f.count;
                f.type = t;
                // The handle itself (m_bus in m_bus.addr) is not a use of this class's data.
                if (recv.kind == ExpressionKind::NamedValue) return;
            }
        }
        visitDefault(e);
    }

    // Local declarations' initializers are not part of the statement walk.
    void handle(const VariableDeclStatement& s) {
        if (const Expression* init = s.symbol.getInitializer()) init->visit(*this);
        visitDefault(s);
    }

    void handle(const NamedValueExpression& e) {
        // An unqualified name of a property of this class or a base.
        if (e.symbol.kind == SymbolKind::ClassProperty &&
            e.symbol.as<ClassPropertySymbol>().lifetime != VariableLifetime::Static) {
            ++own;
        }
        visitDefault(e);
    }

private:
    // Only objects with behaviour of their own. Transactions and config objects are data
    // that components are meant to read; the same class is not "another" class.
    bool counts(const ClassType& t) const {
        if (&t.getCanonicalType() == &self.getCanonicalType() || swe::isLibrarySymbol(t)) return false;
        const swe::Role role = swe::classRole(t);
        if (role == swe::Role::Item || role == swe::Role::Config || role == swe::Role::Sequence) return false;
        if (swe::derivesFrom(self, t) || swe::derivesFrom(t, self)) return false;
        return !swe::classMethods(t).empty();
    }

    const ClassType& self;
};

bool envyExempt(std::string_view name) {
    static const std::set<std::string_view> exempt = {
        "new", "do_copy", "do_compare", "do_print", "do_pack", "do_unpack", "do_record", "copy",
        "compare", "convert2string", "build_phase", "connect_phase", "end_of_elaboration_phase"};
    return exempt.count(name) > 0;
}

} // namespace

class FeatureEnvy : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-011"; }
    std::string_view getName() const override { return "FeatureEnvy"; }
    std::string_view getDescription() const override {
        return "Flags a method that uses one other object's fields at least 6 times (option min_foreign) "
               "and more than twice as often as its own class's fields. The logic belongs with the data "
               "it uses (Fowler: Feature Envy; MG-3.1). Transactions, config objects, the same class and "
               "classes in the same hierarchy do not count, nor do copy/compare/print and build/connect "
               "methods. Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        co_await ctx.standaloneJob();
        const int minForeign = static_cast<int>(swe::ruleOption(getId(), "min_foreign", 6));
        for (const auto* cls : swe::userClasses(ctx.uvmModel())) {
            for (const auto* m : swe::classMethods(*cls)) {
                if (envyExempt(m->name)) continue;
                FieldUseVisitor v(*cls);
                m->getBody().visit(v);
                const std::pair<const std::string, FieldUseVisitor::Foreign>* top = nullptr;
                for (const auto& entry : v.foreign) {
                    if (!top || entry.second.count > top->second.count) top = &entry;
                }
                if (!top || top->second.count < minForeign || top->second.count <= 2 * v.own) continue;

                const std::string other(meth::plainName(*top->second.type));
                ctx.diags().addViolation(
                    getId(), getName(),
                    "Method '" + std::string(m->name) + "' of '" + std::string(cls->name) + "' uses " +
                        std::to_string(top->second.count) + " fields of '" + top->first + "' (class '" +
                        other + "') and " + std::to_string(v.own) +
                        " of its own. The logic belongs with that data: move it into '" + other +
                        "' as a method and call it from here.",
                    core::Severity::Info, m->location);
            }
        }
    }
};

// SV-DBC-001 ------------------------------------------------------------------------------

namespace {

using slang::syntax::SyntaxKind;
using slang::syntax::SyntaxNode;

void scanKinds(const SyntaxNode& node, bool& protocol, bool& assertions) {
    switch (node.kind) {
        case SyntaxKind::ClockingDeclaration:
        case SyntaxKind::ModportDeclaration: protocol = true; break;
        case SyntaxKind::AssertPropertyStatement:
        case SyntaxKind::AssumePropertyStatement:
        case SyntaxKind::CoverPropertyStatement:
        case SyntaxKind::ImmediateAssertStatement:
        case SyntaxKind::PropertyDeclaration:
        case SyntaxKind::CheckerInstantiation: assertions = true; break;
        default: break;
    }
    for (size_t i = 0; i < node.getChildCount(); ++i) {
        if (const SyntaxNode* child = node.childNode(i)) scanKinds(*child, protocol, assertions);
    }
}

std::string trimmed(std::string s) {
    s.erase(0, s.find_first_not_of(" \t\r\n"));
    s.erase(s.find_last_not_of(" \t\r\n") + 1);
    return s;
}

class InterfaceAssertionVisitor : public engine::SyntaxVisitor<InterfaceAssertionVisitor> {
public:
    struct Candidate {
        std::string name;
        slang::SourceLocation loc;
    };
    std::vector<Candidate> candidates;
    std::set<std::string> bound; // targets of bind directives

    void handle(const slang::syntax::ModuleDeclarationSyntax& decl) {
        if (decl.kind == SyntaxKind::InterfaceDeclaration) {
            bool protocol = false, assertions = false;
            scanKinds(decl, protocol, assertions);
            if (protocol && !assertions) {
                candidates.push_back({std::string(decl.header->name.valueText()), decl.header->name.location()});
            }
        }
        visitDefault(decl);
    }

    void handle(const slang::syntax::BindDirectiveSyntax& bind) {
        bound.insert(trimmed(bind.target->toString()));
        visitDefault(bind);
    }
};

} // namespace

class InterfaceWithoutAssertions : public Rule {
public:
    std::string_view getId() const override { return "SV-DBC-001"; }
    std::string_view getName() const override { return "InterfaceWithoutAssertions"; }
    std::string_view getDescription() const override {
        return "Flags an interface with clocking blocks or modports, so a protocol interface, that holds "
               "no assertions (assert/assume/cover property, immediate asserts, property declarations or "
               "checkers) and is not the target of a bind. Every bench that reuses it then re-checks the "
               "protocol by hand, or not at all (design by contract; MG-8.2). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        InterfaceAssertionVisitor visitor;
        co_await ctx.visitSyntax(visitor);
        const auto* sm = ctx.compilation().getSourceManager();
        for (const auto& c : visitor.candidates) {
            if (visitor.bound.count(trimmed(c.name))) continue;
            if (sm && meth::isLibraryBuffer(*sm, c.loc.buffer())) continue;
            ctx.diags().addViolation(
                getId(), getName(),
                "Interface '" + c.name + "' defines a protocol (clocking blocks or modports) but holds no "
                "assertions. Every testbench that reuses it re-checks the protocol by hand, or not at "
                "all. Write the protocol's rules into it as assert property statements, or bind a "
                "checker to it.",
                core::Severity::Info, c.loc);
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNoExtensionPoint() { return std::make_unique<methodology::NoExtensionPoint>(); }
std::unique_ptr<Rule> createFeatureEnvy() { return std::make_unique<methodology::FeatureEnvy>(); }
std::unique_ptr<Rule> createInterfaceWithoutAssertions() {
    return std::make_unique<methodology::InterfaceWithoutAssertions>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createNoExtensionPoint)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createFeatureEnvy)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createInterfaceWithoutAssertions)
