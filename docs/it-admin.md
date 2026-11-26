# Kusema for IT administrators

Kusema reads a few machine-wide settings that an administrator sets for everyone who uses a computer. They're Windows policy values, so you can deploy them with Group Policy (Registry preferences), Intune (a PowerShell script or a custom configuration profile) or any tool that writes the registry. Users without administrator rights can't change them.

Kusema reads them when it starts: after changing one, restart Kusema (or sign out and back in).

## Where

```
HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Neulinc\Kusema
```

| Value              | Type        | Effect                                                                                                                                                                                                                                                                                                                                                                                                               |
| ------------------ | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DisableWebAccess` | `REG_DWORD` | `1`: no web search and no link reading, whatever the user's settings. The Web search settings say they're turned off by the administrator, and a question that asks for the web says the same. Questions to the AI still work.                                                                                                                                                                                       |
| `HiddenProviders`  | `REG_SZ`    | AI providers to hide, by id, separated by commas. Hidden providers don't appear in the provider lists and can't be chosen. One a user had already chosen stays visible so they can switch away from it, and a question to it says it's turned off by the administrator. Ids: `ollama`, `lmstudio`, `anthropic`, `deepseek`, `gemini`, `openai`, `zai`, `openrouter`, `groq`, `cerebras`, `bedrock_mantle`, `custom`. |
| `SearxngUrl`       | `REG_SZ`    | The firm's SearXNG server, for example `https://search.yourfirm.example`. Used for every SearXNG search and shown as set by the administrator; the user can't change it. Only `http://` and `https://` addresses are accepted. See [Running a private SearXNG server](searxng-hosting.md).                                                                                                                           |

Values that aren't set change nothing.

## Examples

Turn off web access on this computer (run as administrator):

```powershell
New-Item -Path 'HKLM:\SOFTWARE\Policies\Neulinc\Kusema' -Force | Out-Null
Set-ItemProperty -Path 'HKLM:\SOFTWARE\Policies\Neulinc\Kusema' -Name 'DisableWebAccess' -Type DWord -Value 1
```

Hide DeepSeek and Gemini, and set the firm's search server:

```powershell
New-Item -Path 'HKLM:\SOFTWARE\Policies\Neulinc\Kusema' -Force | Out-Null
Set-ItemProperty -Path 'HKLM:\SOFTWARE\Policies\Neulinc\Kusema' -Name 'HiddenProviders' -Type String -Value 'deepseek,gemini'
Set-ItemProperty -Path 'HKLM:\SOFTWARE\Policies\Neulinc\Kusema' -Name 'SearxngUrl' -Type String -Value 'https://search.yourfirm.example'
```

Remove all of them:

```powershell
Remove-Item -Path 'HKLM:\SOFTWARE\Policies\Neulinc\Kusema' -Recurse
```

## How it relates to Privileged mode

Privileged mode is the user's own switch: it blocks cloud AI and all web access while it's on. `DisableWebAccess` is the firm's: it blocks web access always, with Privileged mode on or off. Both are enforced where Kusema's network requests are made, not only in the settings screens, and every refused request is listed on the Privacy page.

## What the web features send

When web search is on, Kusema sends search text to the search provider the user chose (Brave Search with the user's key, a SearXNG server, or the AI provider's built-in search), and reads a page only when the user names its address. Every search (provider, search text, number of results, time) and every page read (address, bytes received) is listed on the Privacy page. That list is stored only on the computer, in the same local database as the rest of the privacy ledger: it follows the ledger's retention setting, and the user's **Clear** removes it with everything else. Link reading only reaches public web pages: never this computer, the office network or link-local addresses, checked again after every redirect.
