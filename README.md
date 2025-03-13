# mspm0-data

`mspm0-data` aims to produce clean machine-readable data about MSPM0 microcontroller families, including:

- ✔️ Base chip information
  - Packages
  - 🚧 Flash, RAM size
- ✔️ Peripheral addresses and interrupts
- ✔️ Interrupts
- ✔️ GPIO peripheral function (PF) mappings
- 🚧 Register blocks for all peripherals
- 🚧 DMA mappings
- ✔️ Per package pinouts
- 🚧 Links to applicable technical reference manual and datasheet PDFs.

✔️ = done, 🚧 = work in progress, ❌ = to do

The generated JSON files are [available here in the mspm0-data-generated](todo.txt) repo.

# mspm0-metapac

The generated PAC is [available here in the mspm0-data-generated](todo.txt) repo.

# Data sources

These are the data sources currently used.

* SysConfig metadata from Code Composer Studio
  * Packages and package pinouts
  * Mapping from GPIO pin to IOMUX::PINCM register.
  * Peripheral PF (pin function) mappings.
  * Peripheral pin names.
* mspm0-sdk header files
  * Interrupt number, name
  * Peripheral addresses
* mspm0 SVDs: register blocks
* Manually entered
  * IIDX values for interrupts within a `INT_GROUP`
