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

## Using Kusema with Amazon Bedrock

Kusema can use AI models in your firm's own AWS account through Amazon Bedrock. It connects to Bedrock's `bedrock-mantle` endpoint in the region you choose, with an Amazon Bedrock API key. Claude models are reached through Anthropic's Messages API on that endpoint, and other models through the OpenAI-compatible Chat Completions API. In Kusema: **AI Provider → Amazon Bedrock**, pick the region, paste the key, choose a model, then **Test connection**. Requests are listed on the Privacy page under the regional endpoint, for example `bedrock-mantle.us-east-1.api.aws`, and Privileged mode blocks them like any cloud AI. To hide Bedrock, add `bedrock_mantle` to `HiddenProviders`.

What follows reflects AWS's documentation as of November 2026; check the linked pages for changes.

### Regions

Bedrock's Mantle endpoint is available in us-east-1, us-east-2, us-west-2, eu-central-1, eu-north-1, eu-south-1, eu-west-1, eu-west-2, ap-northeast-1, ap-south-1, ap-southeast-2, ap-southeast-3 and sa-east-1, and in AWS GovCloud (US) us-gov-east-1 and us-gov-west-1 with a GovCloud account ([endpoint availability](https://docs.aws.amazon.com/bedrock/latest/userguide/endpoints-region-availability.html)). Which models a region offers varies; Kusema lists what the chosen region's endpoint offers. Depending on the model, AWS may process a request in other regions (global or geographic [cross-region inference](https://docs.aws.amazon.com/bedrock/latest/userguide/cross-region-inference.html)); CloudTrail records the region that processed it.

### Model access

In commercial regions, access to most Bedrock models is enabled on first use, through an AWS Marketplace subscription that the first call starts; the calling identity needs `aws-marketplace:Subscribe`, `aws-marketplace:Unsubscribe` and `aws-marketplace:ViewSubscriptions` for that. Anthropic's use-case form isn't required for Claude models used through the `bedrock-mantle` endpoint. Some models need account-level access from AWS. ([Model access](https://docs.aws.amazon.com/bedrock/latest/userguide/model-access.html))

### API keys

Bedrock has two kinds of API key ([API keys](https://docs.aws.amazon.com/bedrock/latest/userguide/api-keys.html)):

- **Short-term keys (preferred).** Last up to 12 hours (or the session that created them, whichever is shorter), work only in the region where they were created, and carry the permissions of the identity that created them. Create one in the Bedrock console (**API keys → Short-term API keys**) or with AWS's token generator libraries. Rotating means creating a new one when the old one expires; Kusema says so when a key has expired. A single short-term key can't be revoked; to cut off keys early, deny `bedrock-mantle:CallWithBearerToken` to the identity that created them, or invalidate its sessions ([revoking](https://docs.aws.amazon.com/bedrock/latest/userguide/api-keys-revoke.html)).
- **Long-term keys.** Last until an expiry date you set and create an IAM user. AWS recommends them for exploration only. Rotate by resetting (`ResetServiceSpecificCredential`) or replacing the key before it expires, and deactivate or delete keys you no longer use. Limit their lifetime with the `iam:ServiceSpecificCredentialAgeDays` condition (for example 90 days).

Kusema stores the key in Windows Credential Manager, never in its settings file.

### Least-privilege permissions

For a key used only for Kusema, give its identity:

- `bedrock-mantle:CreateInference` and `bedrock-mantle:CallWithBearerToken`, for inference through the Mantle endpoint with an API key. AWS's managed policy **AmazonBedrockMantleInferenceAccess** covers these.
- The three `aws-marketplace` permissions above, if models in your account still need their first-use subscription.

You can restrict keys further with the `bedrock-mantle:bearerTokenType` condition (`SHORT_TERM` or `LONG_TERM`), for example to allow short-term keys only. ([API key permissions](https://docs.aws.amazon.com/bedrock/latest/userguide/api-keys-permissions.html), [AWS managed policies](https://docs.aws.amazon.com/bedrock/latest/userguide/security-iam-awsmanpol.html))

### Budgets and logging

- **Budget alerts:** in AWS Budgets, create a monthly cost budget and add notifications for actual and forecasted spend, by email or Amazon SNS. Notifications can arrive some time after the charge. ([AWS Budgets](https://docs.aws.amazon.com/cost-management/latest/userguide/budgets-managing-costs.html))
- **CloudTrail:** Bedrock records API calls in CloudTrail; API keys travel in request headers and aren't logged. Mantle endpoint calls have their own CloudTrail page ([logging](https://docs.aws.amazon.com/bedrock/latest/userguide/logging-using-cloudtrail.html)). For cross-region requests, the `additionalEventData.inferenceRegion` field shows where a request was processed.
- **Data retention:** AWS states that Amazon Bedrock doesn't share your content with model providers; in its default mode AWS may keep data for safety and abuse-prevention purposes. An account can choose a stricter mode. ([Data retention](https://docs.aws.amazon.com/bedrock/latest/userguide/data-retention.html))
- **Models from other developers:** models such as DeepSeek and Qwen run in Model Deployment Accounts that AWS owns and operates; AWS states that model providers have no access to those accounts or to customer prompts and completions. So a firm can use these models without sending its text to the developer's own servers. Where the text is processed still depends on the region and model (see Regions above). ([Data protection](https://docs.aws.amazon.com/bedrock/latest/userguide/data-protection.html))

### Web search

Claude's built-in web search isn't available on Amazon Bedrock, so with Bedrock, Kusema's web search uses Brave Search or a SearXNG server.

Neulinc can set this up for your firm: [neulinc.com/kusema](https://neulinc.com/kusema).
