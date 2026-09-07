<?php
// Fixed Wasmer sandbox guest: quote(cart) -> adjustments (JSON stdin/stdout).
function quote(array $cart): array
{
    $currency = $cart['currency'];
    $adjustments = [];
    foreach ($cart['lines'] ?? [] as $line) {
        $quantity = (int) $line['quantity'];
        $unitMinor = (int) $line['unit_price']['amount_minor'];
        if ($quantity >= 2) {
            $adjustments[] = [
                'label' => 'volume-discount',
                'amount_minor' => intdiv(-($quantity * $unitMinor), 10),
                'currency' => $currency,
            ];
        }
    }
    return $adjustments;
}

$raw = stream_get_contents(STDIN);
$cart = json_decode($raw, true, 512, JSON_THROW_ON_ERROR);
echo json_encode(quote($cart), JSON_THROW_ON_ERROR);
