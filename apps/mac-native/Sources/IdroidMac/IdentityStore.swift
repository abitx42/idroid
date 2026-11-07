import CryptoKit
import Foundation
import Security

struct MacIdentity {
    static let keychainLabel = "com.idroid.mac-native.identity"

    let identity: SecIdentity
    let certificate: SecCertificate
    let fingerprint: String

    init() throws {
        if let identity = Self.loadIdentity() {
            self = try Self.make(identity: identity)
            return
        }

        let identity = try Self.generateAndImportIdentity()
        self = try Self.make(identity: identity)
    }

    private static func loadIdentity() -> SecIdentity? {
        var result: CFTypeRef?
        let query: [String: Any] = [
            kSecClass as String: kSecClassIdentity,
            kSecAttrLabel as String: keychainLabel,
            kSecReturnRef as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let result,
              CFGetTypeID(result) == SecIdentityGetTypeID() else { return nil }
        return (result as! SecIdentity)
    }

    private static func make(identity: SecIdentity) throws -> MacIdentity {
        var certificate: SecCertificate?
        guard SecIdentityCopyCertificate(identity, &certificate) == errSecSuccess,
              let certificate else {
            throw IdentityError.certificateUnavailable
        }
        let bytes = SecCertificateCopyData(certificate) as Data
        let fingerprint = SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
        return MacIdentity(identity: identity, certificate: certificate, fingerprint: fingerprint)
    }

    private init(identity: SecIdentity, certificate: SecCertificate, fingerprint: String) {
        self.identity = identity
        self.certificate = certificate
        self.fingerprint = fingerprint
    }

    private static func generateAndImportIdentity() throws -> SecIdentity {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("idroid-identity-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let key = directory.appendingPathComponent("key.pem")
        let certificate = directory.appendingPathComponent("certificate.pem")
        let bundle = directory.appendingPathComponent("identity.p12")
        let password = UUID().uuidString + UUID().uuidString

        try runOpenSSL(arguments: [
            // macOS 26's SecPKCS12Import rejects this EC PKCS#12 shape while
            // importing an RSA identity correctly. TLS 1.3 still provides the
            // transport protocol; the certificate key type is independent.
            "req", "-x509", "-newkey", "rsa:2048",
            "-nodes", "-keyout", key.path, "-out", certificate.path, "-days", "3650",
            "-subj", "/CN=iDroid Mac/",
        ])
        try runOpenSSL(arguments: [
            "pkcs12", "-export", "-out", bundle.path, "-inkey", key.path,
            "-in", certificate.path, "-passout", "pass:\(password)",
            "-name", keychainLabel,
        ])

        let data = try Data(contentsOf: bundle)
        var imported: CFArray?
        let options: [String: Any] = [kSecImportExportPassphrase as String: password]
        guard SecPKCS12Import(data as CFData, options as CFDictionary, &imported) == errSecSuccess,
              let items = imported as? [[String: Any]],
              let value = items.first?[kSecImportItemIdentity as String],
              CFGetTypeID(value as CFTypeRef) == SecIdentityGetTypeID() else {
            throw IdentityError.importFailed
        }
        let identity = value as! SecIdentity

        let item: [String: Any] = [
            kSecClass as String: kSecClassIdentity,
            kSecValueRef as String: identity,
            kSecAttrLabel as String: keychainLabel,
        ]
        let status = SecItemAdd(item as CFDictionary, nil)
        guard status == errSecSuccess || status == errSecDuplicateItem else {
            throw IdentityError.keychainWriteFailed(status)
        }
        return loadIdentity() ?? identity
    }

    private static func runOpenSSL(arguments: [String]) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/openssl")
        process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw IdentityError.opensslFailed
        }
    }
}

enum IdentityError: LocalizedError {
    case certificateUnavailable
    case importFailed
    case keychainWriteFailed(OSStatus)
    case opensslFailed

    var errorDescription: String? {
        switch self {
        case .certificateUnavailable: return "Mac certificate is unavailable"
        case .importFailed: return "Mac identity import failed"
        case .keychainWriteFailed(let status): return "Mac Keychain write failed (\(status))"
        case .opensslFailed: return "Mac identity generation failed"
        }
    }
}

struct PairedMacPeer: Codable {
    let fingerprint: String
    let name: String
    let certificateBase64: String
    let pairedAt: Int64
}

final class MacTrustStore {
    private let service = "com.idroid.mac-native.peer"
    private let account = "android"

    func load() -> PairedMacPeer? {
        var result: CFTypeRef?
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data else { return nil }
        return try? JSONDecoder().decode(PairedMacPeer.self, from: data)
    }

    func save(_ peer: PairedMacPeer) throws {
        let data = try JSONEncoder().encode(peer)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let values: [String: Any] = [kSecValueData as String: data]
        let status = SecItemUpdate(query as CFDictionary, values as CFDictionary)
        if status == errSecItemNotFound {
            var item = query
            item[kSecValueData as String] = data
            guard SecItemAdd(item as CFDictionary, nil) == errSecSuccess else {
                throw IdentityError.keychainWriteFailed(status)
            }
        } else if status != errSecSuccess {
            throw IdentityError.keychainWriteFailed(status)
        }
    }

    func forget() throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw IdentityError.keychainWriteFailed(status)
        }
    }
}
