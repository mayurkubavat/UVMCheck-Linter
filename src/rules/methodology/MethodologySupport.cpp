#include "MethodologySupport.h"

#include <cctype>

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::meth {

using namespace slang::ast;

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

bool isLibraryBuffer(const slang::SourceManager& sm, slang::BufferID buffer) {
    for (int guard = 0; buffer && guard < 64; guard++) {
        std::string_view path = sm.getRawFileName(buffer);
        size_t slash = path.find_last_of("/\\");
        std::string_view base = slash == std::string_view::npos ? path : path.substr(slash + 1);
        if (base.starts_with("uvm_")) return true;
        slang::SourceLocation from = sm.getIncludedFrom(buffer);
        if (!from) break;
        buffer = from.buffer();
    }
    return false;
}

bool isLibraryTree(const slang::SourceManager& sm, const slang::syntax::SyntaxTree& tree) {
    auto ids = tree.getSourceBufferIds();
    if (ids.empty()) return false;
    for (auto id : ids) {
        if (!isLibraryBuffer(sm, id)) return false;
    }
    return true;
}

std::vector<const slang::syntax::SyntaxTree*> userSyntaxTrees(Compilation& comp) {
    std::vector<const slang::syntax::SyntaxTree*> out;
    const auto* sm = comp.getSourceManager();
    if (!sm) return out;
    for (const auto& tree : comp.getSyntaxTrees()) {
        if (tree && !isLibraryTree(*sm, *tree)) out.push_back(tree.get());
    }
    return out;
}

bool isUserLocation(const slang::SourceManager& sm, slang::SourceLocation loc) {
    if (!loc.valid()) return false;
    loc = sm.getFullyOriginalLoc(loc);
    return loc.valid() && !isLibraryBuffer(sm, loc.buffer());
}

bool overridesBaseMethod(const SubroutineSymbol& sub) { return sub.getOverride() != nullptr; }

std::string_view declaringClassName(const CallExpression& call) {
    if (call.isSystemCall()) return {};
    const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine);
    if (!sub || !*sub) return {};
    const Scope* scope = (*sub)->getParentScope();
    if (!scope) return {};
    const Symbol& owner = scope->asSymbol();
    if (owner.kind != SymbolKind::ClassType) return {};
    return plainName(owner.as<ClassType>());
}

bool isFactoryCreate(const CallExpression& call) {
    if (call.getSubroutineName() != "create") return false;
    std::string_view owner = declaringClassName(call);
    return owner == "uvm_component_registry" || owner == "uvm_object_registry";
}

bool isConfigDbCall(const CallExpression& call, std::string_view method) {
    return call.getSubroutineName() == method && declaringClassName(call) == "uvm_config_db";
}

const ClassType* classOfType(const Type& type) {
    const Type* t = &type.getCanonicalType();
    int guard = 0;
    while (t->isUnpackedArray() && guard++ < 8) {
        const Type* elem = t->getArrayElementType();
        if (!elem) break;
        t = &elem->getCanonicalType();
    }
    return swe::asClass(*t);
}

swe::Role roleOfType(const Type& type) {
    const ClassType* cls = classOfType(type);
    return cls ? swe::classRole(*cls) : swe::Role::None;
}

std::string_view plainName(const ClassType& cls) {
    return cls.genericClass ? cls.genericClass->name : cls.name;
}

std::vector<const ClassPropertySymbol*> ownProperties(const ClassType& cls) {
    std::vector<const ClassPropertySymbol*> out;
    for (const auto& m : cls.members()) {
        if (m.kind == SymbolKind::ClassProperty) out.push_back(&m.as<ClassPropertySymbol>());
    }
    return out;
}

bool mentionsActive(const Expression& cond) {
    std::string text = swe::exprText(cond);
    for (char& c : text) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    return text.find("active") != std::string::npos || text.find("passive") != std::string::npos;
}

namespace {

// if (<tests active>) return; ... : everything after it only runs in one mode.
bool isActiveEarlyExit(const Statement& s) {
    if (s.kind != StatementKind::Conditional) return false;
    const auto& c = s.as<ConditionalStatement>();
    if (c.ifFalse || c.conditions.size() != 1) return false;
    return mentionsActive(*c.conditions[0].expr) && swe::alwaysExits(c.ifTrue);
}

class CreationVisitor : public ASTVisitor<CreationVisitor, VisitFlags::AllGood> {
public:
    std::vector<Creation> found;

    void handle(const ConditionalStatement& c) {
        bool saved = guarded;
        for (const auto& cond : c.conditions) {
            cond.expr->visit(*this);
            if (mentionsActive(*cond.expr)) guarded = true;
        }
        c.ifTrue.visit(*this);
        if (c.ifFalse) c.ifFalse->visit(*this);
        guarded = saved;
    }

    void handle(const CaseStatement& c) {
        bool saved = guarded;
        c.expr.visit(*this);
        if (mentionsActive(c.expr)) guarded = true;
        for (const auto& item : c.items) item.stmt->visit(*this);
        if (c.defaultCase) c.defaultCase->visit(*this);
        guarded = saved;
    }

    void handle(const StatementList& list) {
        bool saved = guarded;
        for (const auto* s : list.list) {
            s->visit(*this);
            if (isActiveEarlyExit(*s)) guarded = true;
        }
        guarded = saved;
    }

    void handle(const CallExpression& call) {
        if (isFactoryCreate(call) && call.type) add(*call.type, swe::locOf(call));
        visitDefault(call);
    }

    void handle(const NewClassExpression& e) {
        if (e.type) add(*e.type, swe::locOf(e));
        visitDefault(e);
    }

private:
    bool guarded = false;

    void add(const Type& t, slang::SourceLocation loc) {
        const ClassType* cls = swe::asClass(t);
        if (!cls) return;
        found.push_back({cls, loc, guarded});
    }
};

class CompareCallFinder : public ASTVisitor<CompareCallFinder, VisitFlags::AllGood> {
public:
    bool found = false;
    void handle(const CallExpression& call) {
        if (!call.isSystemCall() && call.getSubroutineName() == "compare") found = true;
        visitDefault(call);
    }
};

bool hasCompareCall(const Expression& e) {
    CompareCallFinder f;
    e.visit(f);
    return f.found;
}

bool isZero(const Expression& e) {
    const Expression& x = swe::stripConversions(e);
    if (!swe::isConstantExpr(x)) return false;
    return swe::exprText(x) == "0" || swe::exprText(x) == "1'b0" || swe::exprText(x) == "'0";
}

std::string lower(std::string s) {
    for (char& c : s) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    return s;
}

bool namesExpected(const std::string& t) {
    return t.find("exp") != std::string::npos || t.find("ref") != std::string::npos ||
           t.find("pred") != std::string::npos;
}

bool namesActual(const std::string& t) {
    return t.find("act") != std::string::npos || t.find("obs") != std::string::npos ||
           t.find("got") != std::string::npos || t.find("rcv") != std::string::npos ||
           t.find("dut") != std::string::npos;
}

// Expected-versus-actual operands, in either order.
bool isExpectedVsActual(const BinaryExpression& b) {
    std::string l = lower(swe::exprText(b.left()));
    std::string r = lower(swe::exprText(b.right()));
    return (namesExpected(l) && namesActual(r)) || (namesActual(l) && namesExpected(r));
}

// Which branch of `if (cond)` runs when the check fails: +1 then, -1 else, 0 not a check.
int failingBranch(const Expression& condExpr, std::string& what) {
    const Expression& cond = swe::stripConversions(condExpr);
    if (cond.kind == ExpressionKind::UnaryOp) {
        const auto& u = cond.as<UnaryExpression>();
        if (u.op == UnaryOperator::LogicalNot && hasCompareCall(u.operand())) {
            what = "compare()";
            return +1;
        }
        return 0;
    }
    if (cond.kind == ExpressionKind::Call) {
        if (hasCompareCall(cond)) {
            what = "compare()";
            return -1;
        }
        return 0;
    }
    if (cond.kind != ExpressionKind::BinaryOp) return 0;
    const auto& b = cond.as<BinaryExpression>();
    bool eq = b.op == BinaryOperator::Equality || b.op == BinaryOperator::CaseEquality;
    bool ne = b.op == BinaryOperator::Inequality || b.op == BinaryOperator::CaseInequality;
    if (!eq && !ne) return 0;
    bool cmpLeft = hasCompareCall(b.left()), cmpRight = hasCompareCall(b.right());
    if ((cmpLeft && isZero(b.right())) || (cmpRight && isZero(b.left()))) {
        what = "compare()";
        return eq ? +1 : -1;
    }
    if (isExpectedVsActual(b)) {
        what = "'" + swe::exprText(cond) + "'";
        return ne ? +1 : -1;
    }
    return 0;
}

class FailureVisitor : public ASTVisitor<FailureVisitor, VisitFlags::AllGood> {
public:
    std::vector<FailureBranch> found;

    void handle(const ConditionalStatement& c) {
        if (!swe::isReportGuard(c) && c.conditions.size() == 1) {
            std::string what;
            int side = failingBranch(*c.conditions[0].expr, what);
            const Statement* branch = side > 0 ? &c.ifTrue : side < 0 ? c.ifFalse : nullptr;
            if (branch) found.push_back({branch, c.conditions[0].expr, what});
        }
        visitDefault(c);
    }
};

} // namespace

std::vector<Creation> creationsIn(const Statement& stmt) {
    CreationVisitor v;
    stmt.visit(v);
    return std::move(v.found);
}

std::vector<Creation> creationsIn(const ClassType& cls) {
    std::vector<Creation> out;
    for (const auto* m : swe::classMethods(cls)) {
        auto found = creationsIn(m->getBody());
        out.insert(out.end(), found.begin(), found.end());
    }
    return out;
}

std::vector<FailureBranch> failureBranches(const Statement& stmt) {
    FailureVisitor v;
    stmt.visit(v);
    return std::move(v.found);
}

} // namespace uvmcheck::rules::meth
