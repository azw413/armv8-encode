import Foundation

public protocol Greeter {
    func greet() -> String
    func farewell() -> String
}

public struct Point {
    public var x: Double
    public var y: Double
}

public enum Mood {
    case happy
    case sad
    case neutral
}

public class Greeting: Greeter {
    public var name: String
    public var mood: Mood
    public var origin: Point

    public init(name: String, mood: Mood, origin: Point) {
        self.name = name
        self.mood = mood
        self.origin = origin
    }

    public func greet() -> String {
        return "Hello, \(name)"
    }

    public func farewell() -> String {
        return "Goodbye, \(name)"
    }

    public func describe() -> String {
        return "\(name) at \(origin.x),\(origin.y)"
    }
}

public class FancyGreeting: Greeting {
    public var emphasis: Int

    public init(name: String, mood: Mood, origin: Point, emphasis: Int) {
        self.emphasis = emphasis
        super.init(name: name, mood: mood, origin: origin)
    }

    public override func greet() -> String {
        return "Hello!!! \(name)"
    }
}
