# Fixed Wasmer sandbox guest: quote(cart) -> adjustments (JSON stdin/stdout).
import json
import sys


def quote(cart):
    currency = cart["currency"]
    adjustments = []
    for line in cart.get("lines", []):
        quantity = int(line["quantity"])
        unit_minor = int(line["unit_price"]["amount_minor"])
        if quantity >= 2:
            amount_minor = -(quantity * unit_minor) // 10
            adjustments.append(
                {
                    "label": "volume-discount",
                    "amount_minor": amount_minor,
                    "currency": currency,
                }
            )
    return adjustments


if __name__ == "__main__":
    cart = json.load(sys.stdin)
    json.dump(quote(cart), sys.stdout)
