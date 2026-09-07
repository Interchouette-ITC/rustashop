<?php
// PrestaShop-inspired migration guest: one hook family → domain event draft.
// Host still commits; this guest only proposes.
function bridge(array $input): array
{
    $hook = $input['hook'] ?? '';
    if ($hook !== 'actionCartUpdateQuantityBefore') {
        throw new RuntimeException('unsupported hook: ' . $hook);
    }
    $operator = $input['operator'] ?? '';
    if (!in_array($operator, ['up', 'down', 'set'], true)) {
        throw new RuntimeException('unsupported operator: ' . $operator);
    }
    return [
        'event_type' => 'cart.line_quantity_proposed',
        'cart_id' => (string) ($input['cart_id'] ?? ''),
        'product_id' => (string) ($input['id_product'] ?? ''),
        'quantity' => (int) ($input['quantity'] ?? 0),
        'operator' => $operator,
    ];
}

$raw = stream_get_contents(STDIN);
$input = json_decode($raw, true, 512, JSON_THROW_ON_ERROR);
echo json_encode(bridge($input), JSON_THROW_ON_ERROR);
