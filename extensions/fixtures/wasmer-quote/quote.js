// Fixed Wasmer sandbox guest: quote(cart) -> adjustments (JSON stdin/stdout).
// Runs under Wasmer QuickJS (`qjs --std -e`); `std` comes from `--std`.
function quote(cart) {
  var currency = cart.currency;
  var adjustments = [];
  var lines = cart.lines || [];
  for (var i = 0; i < lines.length; i++) {
    var line = lines[i];
    var quantity = line.quantity | 0;
    var unitMinor = line.unit_price.amount_minor | 0;
    if (quantity >= 2) {
      adjustments.push({
        label: "volume-discount",
        amount_minor: -Math.trunc((quantity * unitMinor) / 10),
        currency: currency,
      });
    }
  }
  return adjustments;
}

var cart = JSON.parse(std.in.readAsString());
console.log(JSON.stringify(quote(cart)));
