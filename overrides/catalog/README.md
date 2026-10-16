# Local PlayFab catalog items

Put one JSON response in this folder per item: `<UUID>.json`.

The request body to POST /Catalog/GetPublishedItem must contain exactly that UUID
in `itemid` (or `itemId`). The local response must contain the same UUID at
`data.Item.Id`, otherwise Cosmetics keeps the official response unchanged.

The JSON should point to a separately hosted HTTPS `personabinary` ZIP under
`data.Item.Contents[].Url`. This module does NOT fabricate Persona signatures
or skip game resource verification. Using a local path or a fake HTTPS domain as
`Url` alone does not make the game download the file.

Never include authentication tokens, server receipts, account inventory exports,
or other users' data in this public repository.
