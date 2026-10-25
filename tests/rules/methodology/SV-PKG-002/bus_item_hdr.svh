// SV-PKG-002 helper: an import hidden in a file included into a package.
import bus_types_pkg::*; // VIOLATION
class bus_item;
  addr_t addr;
endclass
