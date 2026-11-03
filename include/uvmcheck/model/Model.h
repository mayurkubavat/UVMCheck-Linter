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
    /// The property's type, when it is a class in this model.
    ClassId typeClass = NoClass;
};

struct Method {
    std::string name;
    Loc loc;
    Visibility visibility = Visibility::Public;
    bool isTask = false;
    bool isVirtual = false;
    bool isPure = false;
    bool isStatic = false;
    PhaseKind phase = PhaseKind::None; ///< a standard UVM phase method
};

struct Class {
    std::string name;
    std::string scope; ///< enclosing package or class, "" at $unit
    Loc loc;
    std::string baseName; ///< "" when none
    ClassId base = NoClass;
    UvmKind uvm = UvmKind::None;
    bool isVirtual = false;
    bool isSpecialization = false; ///< a parameterized class's specialization
    /// True for classes from a library path (uvm-core and the like).
    bool isLibrary = false;
    std::vector<Property> properties;
    std::vector<Method> methods;
};

class Model {
public:
    const std::vector<Class>& classes() const { return classes_; }
    const Class& cls(ClassId id) const { return classes_.at(id); }
    std::optional<ClassId> findClass(std::string_view name) const;

    const std::vector<std::string>& files() const { return files_; }
    std::string_view fileName(const Loc& loc) const { return files_.at(loc.file); }

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
