function Invoke-AiMemoryHook {
    param(
        [Parameter(Mandatory = $true)] [string] $Event,
        [Parameter(Mandatory = $true)] [string] $Agent,
        [switch] $FetchHandoff
    )

    $Server = if ($env:AI_MEMORY_HOOK_URL) { $env:AI_MEMORY_HOOK_URL } else { "http://127.0.0.1:49374" }
    $Payload = [Console]::In.ReadToEnd()
    $Headers = @{}

    if ($env:AI_MEMORY_AUTH_TOKEN) {
        $Headers["Authorization"] = "Bearer $env:AI_MEMORY_AUTH_TOKEN"
    }

    try {
        Invoke-WebRequest `
            -UseBasicParsing `
            -TimeoutSec 1 `
            -Method Post `
            -Uri "$Server/hook?event=$Event&agent=$Agent" `
            -Headers $Headers `
            -ContentType "application/json" `
            -Body $Payload | Out-Null
    } catch {
    }

    if ($FetchHandoff) {
        try {
            $Response = Invoke-WebRequest `
                -UseBasicParsing `
                -TimeoutSec 1 `
                -Uri "$Server/handoff?agent=$Agent" `
                -Headers $Headers
            if ($null -ne $Response -and $null -ne $Response.Content) {
                [Console]::Out.Write($Response.Content)
            }
        } catch {
        }
    }
}
