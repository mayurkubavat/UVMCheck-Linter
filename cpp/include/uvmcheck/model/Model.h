#pragma once

// The design model that rules and other DV tools read, with no slang types in
// its interface. It is a plain snapshot (tables of value types indexed by
// small ids), so it can cross the C ABI in Model.h's sibling CApi.h, be
// copied into Rust or Python, and later be filled by a front end other than
// slang without any reader noticing.
//
// Only SlangModel.h knows how a Model is built from slang, and how a Loc maps
// back to a slang location for findings.

#include <cstdint>
#include <optional>
#include <ostream>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

namespace uvmcheck::model {

/// A source position. `raw` is an opaque handle for the backend that made the
/// model (a slang SourceLocation today); file/line/column are already resolved.
struct Loc {
    uint64_t raw = 0;
    uint32_t file = 0; ///< index into Model::files(); 0 is the empty name
    uint32_t line = 0;
    uint32_t column = 0;
    /// For code from a macro: the file and line of the outermost expansion,
    /// which is where findings on it are reported (column 0). 0 otherwise.
    uint32_t expFile = 0;
    uint32_t expLine = 0;
};

/// What a class is in UVM terms, most specific first.
enum class UvmKind : uint8_t {
    None,
    Object,
    SequenceItem,
    Sequence,
    Component,
    Test,
    Env,
    Agent,
    Driver,
    Monitor,
    Sequencer,
    Scoreboard,
    Subscriber,
};
std::string_view toString(UvmKind kind);
/// Any uvm_component, whatever its role.
inline bool isComponent(UvmKind k) { return k >= UvmKind::Component; }

enum class Visibility : uint8_t { Public, Protected, Local };

enum class PhaseKind : uint8_t { None, Function, Task };

enum class Direction : uint8_t { In, Out, InOut, Ref };

/// The shape of a type, as far as rules care: aggregates that are costly to
/// copy, and the rest.
enum class TypeShape : uint8_t {
    Other,
    DynamicArray,
    Queue,
    AssociativeArray,
    FixedUnpackedArray,
    UnpackedStruct,
};

using ClassId = uint32_t;
inline constexpr ClassId NoClass = UINT32_MAX;

struct Property {
    std::string name;
    std::string type; ///< as slang prints it
    Loc loc;
    Visibility visibility = Visibility::Public;
    bool isStatic = false;
    bool isConst = false;
    bool isRand = false;
    /// A virtual interface, or an array of them.
    bool isVirtualInterface = false;
    /// The property's type, when it is a class in this model.
    ClassId typeClass = NoClass;
};

struct Argument {
    std::string name;
    std::string type;          ///< as declared
    std::string canonicalType; ///< typedefs resolved
    Loc loc;
    Direction direction = Direction::In;
    TypeShape shape = TypeShape::Other;
    ClassId typeClass = NoClass;
};

/// A variable declared directly in a method's body.
struct Local {
    std::string name;
    std::string type;
    Loc loc;
    bool isVirtualInterface = false;
};

/// A call made in a method body, in the order the body is walked; calls
/// nested in arguments and in nested blocks are included.
struct Call {
    std::string name;      ///< as written: "randomize", "$urandom", "get"
    Loc loc;
    /// A system task or function, or a built-in method such as randomize().
    bool isSystem = false;
    /// For a call to a user method: the class (generic name, for a
    /// specialization) or package that declares it; empty otherwise.
    std::string calleeScope;
    bool calleeDeclaredVirtual = false;
    bool calleeBuiltIn = false;
    /// The first argument is the `null` literal, and where it starts.
    bool firstArgIsNull = false;
    Loc firstArgLoc;
    /// The object the method is called on, as written: "m_env.ap" for
    /// m_env.ap.connect(x); empty for an unqualified or `::` call.
    std::string receiver;
    /// Written as super.name(...).
    bool viaSuper = false;
};

/// What the left side of an assignment is rooted in: `x`, `x.f`, `x[i]`.
enum class RootKind : uint8_t { Other, Argument, Local, Property };

/// An assignment in a method body (`=`, `<=`, `+=` and the like), in walk order.
struct Assignment {
    Loc loc;
    bool isNonBlocking = false;
    std::string lhs;  ///< source text of the left side, as written
    std::string rhs;  ///< source text of the right side, as written
    RootKind rootKind = RootKind::Other;
    std::string rootName; ///< the argument, local or property assigned into
};

struct Method {
    std::string name;
    Loc loc;
    Visibility visibility = Visibility::Public;
    bool isTask = false;
    /// Virtual, declared so or by overriding a virtual method.
    bool isVirtual = false;
    /// Has the `virtual` keyword itself.
    bool declaredVirtual = false;
    bool isPure = false;
    bool isStatic = false;
    /// Made by the compiler (randomize, pre_randomize and the like).
    bool isBuiltIn = false;
    /// Declared by a prototype in the class (`extern`, or `pure virtual`);
    /// the body, if any, is outside it. loc points at that body.
    bool isExtern = false;
    PhaseKind phase = PhaseKind::None; ///< a standard UVM phase method
    std::string returnType; ///< "void" for tasks
    bool returnsVoid = true;
    std::vector<Argument> arguments;
    std::vector<Local> locals;
    std::vector<Call> calls;
    std::vector<Assignment> assignments;
    /// Delays, event controls and waits in the body, in walk order.
    std::vector<Loc> timingControls;
    /// super.new(...) is called somewhere in the body.
    bool callsSuperNew = false;
    /// Position of the `super.new(...);` statement among the body's top-level
    /// statements (empty statements not counted), or -1; and where it is.
    int superNewIndex = -1;
    Loc superNewLoc;
    /// Calls named `name`, in body order.
    size_t countCalls(std::string_view name) const;
};

struct Constraint {
    std::string name;
    Loc loc;
};
/// A use of a user macro written inside the class's text (`uvm_object_utils
/// and the like), by the macro's name without the backtick.
struct MacroUse {
    std::string name;
    Loc loc;
};
/// A typedef declared in the class (`type_id` and the like).
struct Typedef {
    std::string name;
    Loc loc;
};

struct Class {
    std::string name;
    std::string scope; ///< enclosing package or class, "" at $unit
    Loc loc;
    std::string baseName; ///< "" when none
    ClassId base = NoClass;
    UvmKind uvm = UvmKind::None;
    bool isVirtual = false;
    /// A parameterized class: one entry for the definition, with members as
    /// declared (types from one of its specializations, so parameter types
    /// appear resolved). Specializations map to this entry.
    bool isParameterized = false;
    /// True for classes from a library path (uvm-core and the like).
    bool isLibrary = false;
    /// An `interface class`.
    bool isInterface = false;
    std::vector<Property> properties;
    std::vector<Method> methods;
    std::vector<Constraint> constraints;
    std::vector<MacroUse> macros;
    std::vector<Typedef> typedefs;

    /// Members declared in this class itself (not inherited), by name.
    const Property* findProperty(std::string_view n) const;
    const Method* findMethod(std::string_view n) const;
};

class Model {
public:
    const std::vector<Class>& classes() const { return classes_; }
    const Class& cls(ClassId id) const { return classes_.at(id); }
    std::optional<ClassId> findClass(std::string_view name) const;
    /// The id of a class taken from classes().
    ClassId idOf(const Class& c) const { return static_cast<ClassId>(&c - classes_.data()); }

    const std::vector<std::string>& files() const { return files_; }
    std::string_view fileName(const Loc& loc) const { return files_.at(loc.file); }

    /// The base chain of `id`, nearest first, as far as it is in the model.
    std::vector<ClassId> ancestors(ClassId id) const;

    /// True when `id` is `ancestorName` or derives from it.
    bool derivesFrom(ClassId id, std::string_view ancestorName) const;

    /// Writes the model as JSON (schema "uvmcheck.model/1").
    void writeJson(std::ostream& out) const;

    // Building (backends only).
    ClassId addClass(Class c);
    Class& mutableClass(ClassId id) { return classes_.at(id); }
    uint32_t internFile(std::string_view name);

private:
    std::vector<Class> classes_;
    std::vector<std::string> files_{std::string()};
    std::unordered_map<std::string, ClassId> byName_;   ///< first class of each name
    std::unordered_map<std::string, uint32_t> fileIds_;
};

} // namespace uvmcheck::model
