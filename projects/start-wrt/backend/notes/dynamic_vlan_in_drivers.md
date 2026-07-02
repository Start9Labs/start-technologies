# Dynamic VLAN Tagging for WiFi Access Points

## 1. Introduction

Dynamic VLAN tagging for WiFi Access Points (APs) allows for assigning different clients connecting to the same AP (often with the same SSID) to different VLANs. This is commonly done based on authentication attributes such as RADIUS server responses, MAC address mappings, or user certificate properties, rather than different passwords per VLAN.

**Benefits:**
-   **Network Segmentation:** Isolate different groups of users or devices (e.g., guests, employees, IoT devices) on separate logical networks.
-   **Security:** Enhance security by restricting communication between different segments.
-   **Simplified SSID Management:** Use a single SSID but still achieve network separation.

## 2. Implementation Overview

### 2.1. User-space Configuration

Typically, the assignment of clients to VLANs is managed by a user-space daemon like `hostapd`. `hostapd` can be configured to map specific clients to VLAN IDs through various methods:
- **RADIUS attributes:** A RADIUS server returns VLAN assignment information during authentication.
- **VLAN file mapping:** Static mappings based on MAC addresses or usernames in a local file.
- **Certificate attributes:** VLAN assignment based on properties in client certificates.

When a client associates and authenticates, `hostapd` determines the appropriate VLAN ID and then relies on the kernel and WiFi driver to handle the VLAN-tagged traffic.

### 2.2. Kernel and Driver Role

For dynamic VLANs to function, the kernel's networking stack and the WiFi driver must support and correctly handle VLAN-tagged frames.

-   **Advertising Support:** The WiFi driver, usually via the `mac80211` subsystem, must advertise support for the `NL80211_IFTYPE_AP_VLAN` interface mode. This allows `hostapd` to request the creation of VLAN-specific virtual interfaces (typically named like `wlan0.sta1`, `wlan0.sta2`, etc.) on top of the main AP interface, each associated with a specific VLAN ID.
-   **Packet Handling:**
    -   **Egress (AP to Client):** When the AP sends a frame to a client on a specific VLAN, the Linux bridge (if used) or networking stack adds the 802.1Q VLAN tag. The WiFi driver/firmware must then transmit this frame, ensuring the VLAN tag is handled correctly according to the 802.11 standard (typically stripped before wireless transmission since 802.11 frames don't carry 802.1Q tags over the air).
    -   **Ingress (Client to AP):** When the AP receives a frame from a client, the driver/firmware passes it up. If the client was assigned to a VLAN, the networking stack expects to see frames on the corresponding VLAN virtual interface.
-   **Interaction with `mac80211`:**
    -   The `net/mac80211/main.c` subsystem plays a crucial role. A key piece of logic in `ieee80211_register_hw()` is:
        ```c
        /*
         * drivers advertising SW_CRYPTO_CONTROL should enable AP_VLAN
         * based on their support to transmit SW encrypted packets.
         */
        if (local->hw.wiphy->interface_modes & BIT(NL80211_IFTYPE_AP) &&
            !ieee80211_hw_check(&local->hw, SW_CRYPTO_CONTROL)) {
            hw->wiphy->interface_modes |= BIT(NL80211_IFTYPE_AP_VLAN);
            hw->wiphy->software_iftypes |= BIT(NL80211_IFTYPE_AP_VLAN);
        }
        ```
    - This means if a driver supports AP mode and does **not** set the `IEEE80211_HW_SW_CRYPTO_CONTROL` flag (implying the driver/firmware handles encryption/decryption), `mac80211` will **automatically** enable `NL80211_IFTYPE_AP_VLAN` in the wiphy capabilities. The interface type is added to both `interface_modes` (hardware-supported modes) and `software_iftypes` (modes handled by mac80211 software layer).
    - If `IEEE80211_HW_SW_CRYPTO_CONTROL` **is** set, `mac80211` handles software encryption. In this scenario, VLAN tagging is generally managed by the Linux bridge before packets reach `mac80211` for encryption.

### 2.3. Challenges with Hardware Encryption and VLANs

A significant challenge arises when the driver/firmware handles cryptographic operations (i.e., when the driver does not set the `IEEE80211_HW_SW_CRYPTO_CONTROL` flag).

-   **Unicast Frames:** Handling VLAN-tagged unicast frames might be relatively straightforward if the firmware is designed to process them alongside encryption.
-   **Multicast/Broadcast Frames (e.g., ARP, DHCP, mDNS on WPA2/WPA3 PSK networks):**
    -   These frames are typically encrypted using a Group Temporal Key (GTK) shared among all clients on that specific BSS. In a dynamic VLAN context, each VLAN should ideally have its own GTK to maintain proper isolation, meaning multicast frames need to be encrypted separately for each VLAN.
    -   If the **firmware** encrypts these group frames, it must also correctly handle the associated VLAN tag.
    -   If encryption for these group frames is done in **software** (e.g., by `hostapd` or the kernel), the driver/firmware needs to be explicitly informed that an outgoing frame is already encrypted and should be transmitted "raw" without further modification.
    -   The `ath11k` patch (`906-ath11k-add-support-for-dynamic-vlan.patch`) addresses this specific scenario by adding logic to prepare HTT (Host-to-Target Transfer) metadata for software-encrypted frames.

## 3. How to Determine Driver Support

### 3.1. End-User Check (Runtime)

The most straightforward way for an end-user to check if dynamic VLAN capability is advertised is to use the `iw` utility:

```bash
iw list
```

In the output, look for the "Supported interface modes:" section. If `AP/VLAN` is listed, the driver advertises support.

### 3.2. Driver Source Code Inspection

To determine support by inspecting driver source code:

**1. Explicit AP_VLAN Advertisement:**
   - Check if the driver's wiphy initialization code explicitly adds `BIT(NL80211_IFTYPE_AP_VLAN)`.
   - The `ath11k` patch does this based on a hardware parameter:
     ```c
     // In drivers/net/wireless/ath/ath11k/mac.c (from patch)
     if (ab->hw_params.supports_ap_vlan) {
         ar->hw->wiphy->interface_modes |= BIT(NL80211_IFTYPE_AP_VLAN);
         ar->hw->wiphy->software_iftypes |= BIT(NL80211_IFTYPE_AP_VLAN);
     }
     ```

**2. `IEEE80211_HW_SW_CRYPTO_CONTROL` Flag:**
   - Determine if the driver sets this flag.
   - **If NOT set (Hardware/Firmware Crypto):** `mac80211` will automatically enable AP_VLAN support. The critical aspect is whether the driver/firmware can correctly handle VLAN tags with hardware crypto.
   - **If SET (Software Crypto by `mac80211`):** `mac80211` handles encryption, and this setup is generally more compatible with dynamic VLANs.

**3. Specific Packet Handling for VLANs:**
   - If hardware crypto is used, this is the most important area to check.
   - Search for code that prepares special metadata for VLAN-tagged packets, especially for encrypted multicast/broadcast traffic. The `ath11k` patch's `ath11k_dp_prepare_htt_metadata` function is a key example.

## 4. Driver Examples

### 4.1. ath11k (with patch)
- **Support:** Yes, explicitly enabled by the patch.
- **Indicators:**
    - Introduces a `supports_ap_vlan` flag.
    - Uses this flag in `ath11k_mac_register` to add `BIT(NL80211_IFTYPE_AP_VLAN)`.
    - `ath11k` does not set `SW_CRYPTO_CONTROL`, relying on hardware crypto.
    - The patch adds crucial logic in `dp_tx.c` to create HTT metadata for software-encrypted multicast/broadcast frames, informing the firmware to transmit them "raw".

### 4.2. ath9k
- **Support:** Advertised by `mac80211`, but practical functionality for encrypted multicast is uncertain.
- **Indicators:**
    - `ath9k` does not set the `IEEE80211_HW_SW_CRYPTO_CONTROL` flag.
    - `mac80211` automatically adds `BIT(NL80211_IFTYPE_AP_VLAN)` to the supported modes.
    - However, `ath9k` lacks the explicit metadata preparation for software-encrypted group frames that the `ath11k` patch introduced.
- **Conclusion:** `iw list` will likely show `AP/VLAN` support. Basic VLAN tagging might work, but issues might arise in WPA2/WPA3 PSK environments with multiple VLANs.

### 4.3. brcmfmac
- **Support:** No.
- **Indicators:**
    - The `brcmf_cfg80211_add_iface` function explicitly blocks the creation of `AP_VLAN` interfaces:
      ```c
      case NL80211_IFTYPE_AP_VLAN:
          return ERR_PTR(-EOPNOTSUPP);
      ```

### 4.4. iwlwifi (including ax210)
- **Support:** Advertised by `mac80211`, but with potential practical limitations.
- **Analysis:**
    - **`SW_CRYPTO_CONTROL`**: `iwlwifi` does **NOT** set this flag, meaning it uses hardware/firmware encryption.
    - **AP Mode Support**: `iwlwifi` explicitly supports AP mode:
       ```c
       hw->wiphy->interface_modes = BIT(NL80211_IFTYPE_STATION) |
           BIT(NL80211_IFTYPE_P2P_CLIENT) |
           BIT(NL80211_IFTYPE_AP) |        // <- AP mode supported
           BIT(NL80211_IFTYPE_P2P_GO) |
           BIT(NL80211_IFTYPE_P2P_DEVICE) |
           BIT(NL80211_IFTYPE_ADHOC);
       ```
    - **Automatic AP_VLAN Advertisement**: Since `iwlwifi` supports AP mode and does NOT set `SW_CRYPTO_CONTROL`, `mac80211` automatically advertises `AP_VLAN` support.
    - **Missing VLAN-Specific Handling**: Unlike `ath11k`, `iwlwifi` lacks explicit driver code for handling VLAN tags in conjunction with hardware encryption.
- **Practical Implications:**
    - Basic VLAN tagging for unicast frames may work.
    - **Potential issues** with multicast/broadcast frames (ARP, DHCP) on WPA2/WPA3 PSK networks with multiple VLANs.
    - The ax210 chipset uses the same `iwlwifi` driver, so this analysis applies directly.

## 5. Summary of Key Points

- Dynamic VLAN tagging for APs hinges on the `NL80211_IFTYPE_AP_VLAN` interface type.
- `mac80211` automatically advertises `AP_VLAN` support if the driver uses hardware/firmware encryption (i.e., `SW_CRYPTO_CONTROL` is not set) and supports basic AP mode.
  ```c
  if (local->hw.wiphy->interface_modes & BIT(NL80211_IFTYPE_AP) &&
      !ieee80211_hw_check(&local->hw, SW_CRYPTO_CONTROL)) {
      hw->wiphy->interface_modes |= BIT(NL80211_IFTYPE_AP_VLAN);
      hw->wiphy->software_iftypes |= BIT(NL80211_IFTYPE_AP_VLAN);
  }
  ```
- **Important:** This automatic advertisement does not guarantee full functionality, especially the complex interaction between VLAN tagging and hardware encryption for multicast traffic.
- For drivers using hardware encryption, robust dynamic VLAN functionality requires explicit driver and firmware support.
- **Important:** Advertised `AP/VLAN` support (visible via `iw list`) does not guarantee full functionality. Always test thoroughly, paying special attention to multicast/broadcast traffic (DHCP, ARP, mDNS) across different VLANs on encrypted networks.
