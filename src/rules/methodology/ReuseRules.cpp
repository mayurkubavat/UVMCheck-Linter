// Methodology guide chapter MG-4, reusable verification components: UVM-RUS-001 to UVM-RUS-006.

#include "MethodologySupport.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/text/SourceManager.h"

#include <map>
#include <set>

namespace uvmcheck::rules::methodology {

using namespace slang::ast;

namespace {

bool isStringLiteral(const Expression& e) {
    return swe::stripConversions(e).kind == ExpressionKind::StringLiteral;
}

std::string literalValue(const Expression& e) {
    const Expression& x = swe::stripConversions(e);
    if (x.kind != ExpressionKind::StringLiteral) return {};
    return std::string(x.as<StringLiteral>().getValue());
}

bool isNullLiteral(const Expression& e) {
    return swe::stripConversions(e).kind == ExpressionKind::NullLiteral;
}

std::string_view className(const ClassType& cls) { return meth::plainName(cls); }

} // namespace

// UVM-RUS-001 -----------------------------------------------------------------------------

class AgentIgnoresIsActive : public Rule {
public:
    std::string_view getId() const override { return "UVM-RUS-001"; }
    std::string_view getName() const override { return "AgentIgnoresIsActive"; }
    std::string_view getDescription() const override {
        return "Flags agents that create their driver or sequencer without checking is_active or "
               "get_is_active(). Such an agent cannot be reused as a passive monitor in a larger env "
               "(MG-4.1).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Agent) continue;
            for (const auto& c : meth::creationsIn(*cls)) {
                swe::Role r = swe::classRole(*c.type);
                if (c.guardedByActive || (r != swe::Role::Driver && r != swe::Role::Sequencer)) continue;
                diags.addViolation(
                    "UVM-RUS-001", "AgentIgnoresIsActive",
                    "Agent '" + std::string(cls->name) + "' creates " + swe::roleName(r) + " '" +
                        std::string(className(*c.type)) + "' without checking get_is_active(). In a "
                        "passive agent the driver would still fight the DUT for the bus. Create the "
                        "driver and sequencer only inside 'if (get_is_active() == UVM_ACTIVE)'.",
                    core::Severity::Warning, c.loc);
            }
        }
    }
};

// UVM-RUS-002 -----------------------------------------------------------------------------

class AgentWithoutConfigObject : public Rule {
public:
    std::string_view getId() const override { return "UVM-RUS-002"; }
    std::string_view getName() const override { return "AgentWithoutConfigObject"; }
    std::string_view getDescription() const override {
        return "Flags agents and envs that read three or more separate scalar settings from "
               "uvm_config_db instead of one config object (Fowler 'data clumps', MG-4.2).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("UVM-RUS-002", "max_scalar_fields", 2);
        for (const auto* cls : swe::userClasses(model)) {
            swe::Role role = swe::classRole(*cls);
            if (role != swe::Role::Agent && role != swe::Role::Env) continue;
            std::set<std::string> fields;
            slang::SourceLocation first;
            for (const auto* m : swe::classMethods(*cls)) {
                swe::forEachCall(m->getBody(), [&](const CallExpression& call) {
                    if (!meth::isConfigDbCall(call, "get") || call.arguments().size() < 4) return;
                    const Expression& value = *call.arguments()[3];
                    if (!value.type) return;
                    const Type& t = value.type->getCanonicalType();
                    if (t.isClass() || t.isVirtualInterface() || t.isUnpackedArray()) return;
                    std::string field = isStringLiteral(*call.arguments()[2])
                                            ? literalValue(*call.arguments()[2])
                                            : swe::exprText(*call.arguments()[2]);
                    if (fields.insert(field).second && fields.size() == 1) first = swe::locOf(call);
                });
            }
            if (static_cast<int64_t>(fields.size()) <= limit) continue;
            std::string list;
            for (const auto& f : fields) list += (list.empty() ? "" : ", ") + f;
            diags.addViolation(
                "UVM-RUS-002", "AgentWithoutConfigObject",
                std::string(swe::roleName(role)) + " '" + std::string(cls->name) + "' reads " +
                    std::to_string(fields.size()) + " separate settings from uvm_config_db (" + list +
                    "). Settings that travel together belong in one config object: it is typed, "
                    "documented in one place, can be randomized and is set with one call.",
                core::Severity::Warning, first.valid() ? first : cls->location);
        }
    }
};

// UVM-RUS-003 -----------------------------------------------------------------------------

namespace {

// The literal count of a loop: repeat (4), for (i = 0; i < 4; i++).
std::optional<std::string> literalBound(const Statement& s) {
    auto literal = [](const Expression& e) -> std::optional<std::string> {
        const Expression& x = swe::stripConversions(e);
        if (x.kind != ExpressionKind::IntegerLiteral) return std::nullopt;
        return swe::exprText(x);
    };
    if (s.kind == StatementKind::RepeatLoop) return literal(s.as<RepeatLoopStatement>().count);
    if (s.kind == StatementKind::ForLoop) {
        const auto* stop = s.as<ForLoopStatement>().stopExpr;
        if (!stop) return std::nullopt;
        const Expression& x = swe::stripConversions(*stop);
        if (x.kind != ExpressionKind::BinaryOp) return std::nullopt;
        return literal(x.as<BinaryExpression>().right());
    }
    return std::nullopt;
}

class TopologyVisitor : public ASTVisitor<TopologyVisitor, VisitFlags::AllGood> {
public:
    TopologyVisitor(const ClassType& cls, core::DiagnosticEngine& diags) : cls(cls), diags(diags) {}

    void handle(const RepeatLoopStatement& s) { check(s, s.body); visitDefault(s); }
    void handle(const ForLoopStatement& s) { check(s, s.body); visitDefault(s); }

private:
    const ClassType& cls;
    core::DiagnosticEngine& diags;

    void check(const Statement& loop, const Statement& body) {
        auto bound = literalBound(loop);
        if (!bound) return;
        for (const auto& c : meth::creationsIn(body)) {
            if (!swe::isComponentRole(swe::classRole(*c.type))) continue;
            diags.addViolation(
                "UVM-RUS-003", "HardcodedTopology",
                "Env '" + std::string(cls.name) + "' creates " + *bound + " instances of '" +
                    std::string(className(*c.type)) + "' with a literal loop count. The same env "
                    "cannot then be reused with a different number of interfaces; take the count "
                    "(and per-instance settings) from the env's config object.",
                core::Severity::Warning, swe::locOf(loop));
            return;
        }
    }
};

} // namespace

class HardcodedTopology : public Rule {
public:
    std::string_view getId() const override { return "UVM-RUS-003"; }
    std::string_view getName() const override { return "HardcodedTopology"; }
    std::string_view getDescription() const override {
        return "Flags envs that create components in a loop with a literal count instead of a count "
               "from their config object (open/closed principle, MG-4.2).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Env) continue;
            TopologyVisitor v(*cls, diags);
            for (const auto* m : swe::classMethods(*cls)) m->getBody().visit(v);
        }
    }
};

// UVM-RUS-004 -----------------------------------------------------------------------------

class AbsoluteConfigDbPath : public Rule {
public:
    std::string_view getId() const override { return "UVM-RUS-004"; }
    std::string_view getName() const override { return "AbsoluteConfigDbPath"; }
    std::string_view getDescription() const override {
        return "Flags uvm_config_db calls in classes whose path starts at uvm_test_top. Below the test "
               "the path should be relative to 'this'; with a non-null context an absolute path is "
               "appended to the context and never matches (MG-4.3).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            swe::Role role = swe::classRole(*cls);
            for (const auto* m : swe::classMethods(*cls)) {
                swe::forEachCall(m->getBody(), [&](const CallExpression& call) {
                    bool isSet = meth::isConfigDbCall(call, "set");
                    if ((!isSet && !meth::isConfigDbCall(call, "get")) || call.arguments().size() < 2) return;
                    std::string path = literalValue(*call.arguments()[1]);
                    std::string_view p = path;
                    while (!p.empty() && (p.front() == '*' || p.front() == '.')) p.remove_prefix(1);
                    if (!p.starts_with("uvm_test_top")) return;
                    bool nullContext = isNullLiteral(*call.arguments()[0]);
                    if (nullContext && role == swe::Role::Test) return;
                    std::string msg =
                        nullContext
                            ? std::string(swe::roleName(role)) + " '" + std::string(cls->name) +
                                  "' uses the absolute config_db path \"" + path +
                                  "\". The class then only works at that one place in one test's "
                                  "hierarchy. Use 'this' as the context and a path relative to it."
                            : "config_db " + std::string(isSet ? "set" : "get") + "() in '" +
                                  std::string(cls->name) + "' combines a non-null context with the "
                                  "absolute path \"" + path + "\". UVM appends the path to the "
                                  "context's full name, so it can never match. Pass null as the "
                                  "context, or make the path relative.";
                    diags.addViolation("UVM-RUS-004", "AbsoluteConfigDbPath", msg,
                                       core::Severity::Warning, swe::locOf(call));
                });
            }
        }
    }
};

// UVM-RUS-005 -----------------------------------------------------------------------------

namespace {

// Factory calls that take type names as strings, and which arguments those are.
std::vector<size_t> typeNameArgs(const CallExpression& call) {
    std::string_view owner = meth::declaringClassName(call);
    std::string_view n = call.getSubroutineName();
    if (owner == "uvm_factory") {
        if (n == "set_type_override_by_name" || n == "set_inst_override_by_name") return {0, 1};
        if (n == "create_object_by_name" || n == "create_component_by_name") return {0};
    } else if (owner == "uvm_component") {
        if (n == "set_type_override") return {0, 1};
        if (n == "set_inst_override") return {1, 2};
        if (n == "create_object" || n == "create_component") return {0};
    }
    return {};
}

} // namespace

class OverrideByNameString : public Rule {
public:
    std::string_view getId() const override { return "UVM-RUS-005"; }
    std::string_view getName() const override { return "OverrideByNameString"; }
    std::string_view getDescription() const override {
        return "Flags factory overrides and creates that name types with string literals "
               "(set_type_override_by_name, set_inst_override, create_object ...). A typo or rename "
               "is only found at run time; T::type_id::set_type_override() is checked by the "
               "compiler (connascence of name, MG-4.3).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* sub : swe::allUserRoutines(comp, model)) {
            swe::forEachCall(sub->getBody(), [&](const CallExpression& call) {
                // Only fully literal calls: a name computed at run time has no typed form.
                auto idx = typeNameArgs(call);
                std::string names;
                for (size_t i : idx) {
                    if (i >= call.arguments().size() || !isStringLiteral(*call.arguments()[i])) return;
                    names += (names.empty() ? "\"" : ", \"") + literalValue(*call.arguments()[i]) + "\"";
                }
                if (names.empty()) return;
                diags.addViolation(
                    "UVM-RUS-005", "OverrideByNameString",
                    std::string(call.getSubroutineName()) + "() names types as strings (" + names +
                        "). A misspelt or renamed type is only noticed when the test runs, and often "
                        "not even then. Use the typed form, such as "
                        "base_t::type_id::set_type_override(derived_t::get_type()).",
                    core::Severity::Warning, swe::locOf(call));
            });
        }
    }
};

// UVM-RUS-006 -----------------------------------------------------------------------------

namespace {

std::string_view originalClassText(const ClassType& cls) {
    const auto* syntax = cls.getSyntax();
    const auto* sm = cls.getCompilation().getSourceManager();
    if (!syntax || !sm) return {};
    auto range = syntax->sourceRange();
    auto start = sm->getFullyOriginalLoc(range.start());
    auto end = sm->getFullyOriginalLoc(range.end());
    if (start.buffer() != end.buffer() || end.offset() <= start.offset()) return {};
    std::string_view text = sm->getSourceText(start.buffer());
    if (end.offset() > text.size()) return {};
    return text.substr(start.offset(), end.offset() - start.offset());
}

// Read from the source text: uvm-core's `uvm_object_utils generates the field automation
// method even for classes without `uvm_field_* macros.
bool usesFieldMacros(const ClassType& cls) {
    return originalClassText(cls).find("`uvm_field_") != std::string_view::npos;
}

} // namespace

class IncompleteItemContract : public Rule {
public:
    std::string_view getId() const override { return "UVM-RUS-006"; }
    std::string_view getName() const override { return "IncompleteItemContract"; }
    std::string_view getDescription() const override {
        return "Flags concrete sequence items with fields that implement neither field macros nor "
               "do_copy(), do_compare() and convert2string()/do_print(). Scoreboards, clone() and "
               "debug prints then silently see default behaviour (MG-4.4).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Item || cls->isAbstract) continue;
            bool hasFields = false, macros = false;
            std::set<std::string_view> methods;
            for (const ClassType* c = cls; c && !swe::isLibrarySymbol(*c);) {
                for (const auto* p : meth::ownProperties(*c)) {
                    if (p->lifetime != VariableLifetime::Static) hasFields = true;
                }
                macros |= usesFieldMacros(*c);
                for (const auto* m : swe::classMethods(*c)) methods.insert(m->name);
                const Type* base = c->getBaseClass();
                c = base && base->isClass() ? &base->getCanonicalType().as<ClassType>() : nullptr;
            }
            if (!hasFields || macros) continue;
            std::vector<std::string> missing;
            if (!methods.count("do_copy")) missing.push_back("do_copy()");
            if (!methods.count("do_compare")) missing.push_back("do_compare()");
            if (!methods.count("convert2string") && !methods.count("do_print"))
                missing.push_back("convert2string() or do_print()");
            if (missing.empty()) continue;
            std::string list;
            for (size_t i = 0; i < missing.size(); ++i) {
                list += (i == 0 ? "" : i + 1 == missing.size() ? " and " : ", ") + missing[i];
            }
            diags.addViolation(
                "UVM-RUS-006", "IncompleteItemContract",
                "Sequence item '" + std::string(cls->name) + "' has fields but no " + list +
                    ". Without them copy(), clone() and compare() ignore the fields, so a scoreboard "
                    "comparing two items always passes, and logs print nothing useful.",
                core::Severity::Warning, cls->location);
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createAgentIgnoresIsActive() { return std::make_unique<methodology::AgentIgnoresIsActive>(); }
std::unique_ptr<Rule> createAgentWithoutConfigObject() { return std::make_unique<methodology::AgentWithoutConfigObject>(); }
std::unique_ptr<Rule> createHardcodedTopology() { return std::make_unique<methodology::HardcodedTopology>(); }
std::unique_ptr<Rule> createAbsoluteConfigDbPath() { return std::make_unique<methodology::AbsoluteConfigDbPath>(); }
std::unique_ptr<Rule> createOverrideByNameString() { return std::make_unique<methodology::OverrideByNameString>(); }
std::unique_ptr<Rule> createIncompleteItemContract() { return std::make_unique<methodology::IncompleteItemContract>(); }
}
